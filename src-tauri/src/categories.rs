use scrat_application::category_service::CategoryService;
use scrat_domain::category::{Category, CategoryId};
use scrat_domain::ports::CategoryRepository;
use scrat_infra_sqlite::{Connection, SqliteCategoryRepository};
use serde::Serialize;
use tauri::State;

use crate::db::DbState;

#[derive(Debug, Serialize)]
pub struct CategoryDto {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    /// Icon key (e.g. "house") — always `Some` for a top-level category,
    /// always `None` for a subcategory. See `scrat_domain::category::CATEGORY_ICONS`
    /// for the closed set of valid keys.
    pub icon: Option<String>,
    /// Whether this is the app-wide default "Uncategorized" category — the
    /// one new transactions fall back to when none is explicitly chosen
    /// (e.g. an unset "category for all rows" during CSV import). Forced,
    /// not user-selectable — this category can never be renamed or deleted.
    pub is_default: bool,
}

fn to_dto(category: Category, default_category_id: CategoryId) -> CategoryDto {
    CategoryDto {
        id: category.id().as_string(),
        name: category.name().as_str().to_string(),
        parent_id: category.parent_id().map(|p| p.as_string()),
        icon: category.icon().map(|i| i.as_str().to_string()),
        is_default: category.id() == default_category_id,
    }
}

fn parse_id(id: &str) -> Result<CategoryId, String> {
    CategoryId::parse(id).map_err(|e| e.to_string())
}

fn parse_optional_id(id: Option<String>) -> Result<Option<CategoryId>, String> {
    id.as_deref().map(parse_id).transpose()
}

fn with_service<T>(
    state: &State<DbState>,
    f: impl FnOnce(&CategoryService) -> Result<T, scrat_application::category_service::ApplicationError>,
) -> Result<T, String> {
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let repo = SqliteCategoryRepository::new(conn);
    let service = CategoryService::new(&repo);
    f(&service).map_err(|e| e.to_string())
}

/// Resolves the app-wide default category id: the forced "Uncategorized"
/// category, creating it if this database predates it. Not user-selectable —
/// there's nothing to persist here, since there's only ever one answer.
pub(crate) fn resolve_default_category_id(conn: &Connection) -> Result<CategoryId, String> {
    let repo = SqliteCategoryRepository::new(conn);
    let service = CategoryService::new(&repo);
    let default_category = service
        .get_or_create_default_category()
        .map_err(|e| e.to_string())?;
    Ok(default_category.id())
}

#[tauri::command]
pub fn list_categories(state: State<DbState>) -> Result<Vec<CategoryDto>, String> {
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let repo = SqliteCategoryRepository::new(conn);
    let service = CategoryService::new(&repo);
    let categories = service.list_categories().map_err(|e| e.to_string())?;
    let default_category_id = resolve_default_category_id(conn)?;
    Ok(categories
        .into_iter()
        .map(|c| to_dto(c, default_category_id))
        .collect())
}

#[tauri::command]
pub fn create_category(
    state: State<DbState>,
    name: String,
    parent_id: Option<String>,
) -> Result<CategoryDto, String> {
    let parent_id = parse_optional_id(parent_id)?;
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let repo = SqliteCategoryRepository::new(conn);
    let service = CategoryService::new(&repo);
    let category = service
        .create_category(&name, parent_id)
        .map_err(|e| e.to_string())?;
    let default_category_id = resolve_default_category_id(conn)?;
    Ok(to_dto(category, default_category_id))
}

#[tauri::command]
pub fn rename_category(state: State<DbState>, id: String, name: String) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.rename_category(id, &name))
}

#[tauri::command]
pub fn set_category_icon(state: State<DbState>, id: String, icon: String) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.set_category_icon(id, &icon))
}

#[tauri::command]
pub fn move_category(
    state: State<DbState>,
    id: String,
    parent_id: Option<String>,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    let parent_id = parse_optional_id(parent_id)?;
    with_service(&state, |s| s.move_category(id, parent_id))
}

#[tauri::command]
pub fn delete_category(
    state: State<DbState>,
    id: String,
    reassign_to: Option<String>,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    let reassign_to = parse_optional_id(reassign_to)?;
    with_service(&state, |s| s.delete_category(id, reassign_to))
}

/// Resolves the category the "Mean monthly spend" card treats as rent:
/// whatever's configured in settings, as long as it still exists —
/// otherwise falls back to a category literally named "Rent"
/// (case-insensitive), which was the original, non-configurable heuristic.
/// Returns `None` when neither resolves to anything.
pub(crate) fn resolve_rent_category_id(conn: &Connection) -> Result<Option<CategoryId>, String> {
    let repo = SqliteCategoryRepository::new(conn);
    let service = CategoryService::new(&repo);
    let categories = service.list_categories().map_err(|e| e.to_string())?;

    if let Some(id_str) =
        scrat_infra_sqlite::get_rent_category_id(conn).map_err(|e| e.to_string())?
        && let Ok(id) = CategoryId::parse(&id_str)
        && categories.iter().any(|c| c.id() == id)
    {
        return Ok(Some(id));
    }

    Ok(categories
        .iter()
        .find(|c| c.name().as_str().trim().eq_ignore_ascii_case("rent"))
        .map(|c| c.id()))
}

#[tauri::command]
pub fn get_rent_category(state: State<DbState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    Ok(resolve_rent_category_id(conn)?.map(|id| id.as_string()))
}

#[tauri::command]
pub fn set_rent_category(state: State<DbState>, id: String) -> Result<(), String> {
    let id = parse_id(&id)?;
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    let repo = SqliteCategoryRepository::new(conn);
    repo.find_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "category not found".to_string())?;
    scrat_infra_sqlite::set_rent_category_id(conn, &id.as_string()).map_err(|e| e.to_string())
}
