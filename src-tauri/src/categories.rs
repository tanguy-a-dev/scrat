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

#[cfg(test)]
mod tests {
    use scrat_domain::category::{CategoryIcon, CategoryName};

    use super::*;

    fn category(name: &str, parent_id: Option<CategoryId>) -> Category {
        Category::new(
            CategoryId::new(),
            CategoryName::new(name).unwrap(),
            parent_id,
        )
        .unwrap()
    }

    #[test]
    fn a_top_level_category_dto_carries_every_field_across() {
        let mut housing = category("Housing", None);
        housing
            .set_icon(Some(CategoryIcon::new("house").unwrap()))
            .unwrap();
        let id = housing.id();

        let dto = to_dto(housing, CategoryId::new());

        assert_eq!(dto.id, id.as_string());
        assert_eq!(dto.name, "Housing");
        assert_eq!(dto.parent_id, None);
        assert_eq!(dto.icon.as_deref(), Some("house"));
        assert!(!dto.is_default);
    }

    /// A subcategory carries its parent's id as a string, so the UI can nest
    /// it. Losing this would flatten the two-level hierarchy into one list.
    #[test]
    fn a_subcategory_reports_its_parent() {
        let parent_id = CategoryId::new();

        let dto = to_dto(category("Rent", Some(parent_id)), CategoryId::new());

        assert_eq!(dto.parent_id, Some(parent_id.as_string()));
        assert_eq!(dto.icon, None);
    }

    /// Only the app-wide "Uncategorized" category is flagged — it's the one
    /// the UI must refuse to rename or delete.
    #[test]
    fn only_the_default_category_is_flagged_as_default() {
        let uncategorized = category("Uncategorized", None);
        let default_id = uncategorized.id();

        let default_dto = to_dto(uncategorized, default_id);
        let other_dto = to_dto(category("Housing", None), default_id);

        assert!(default_dto.is_default);
        assert!(!other_dto.is_default);
    }

    #[test]
    fn an_iconless_top_level_category_reports_no_icon() {
        let dto = to_dto(category("Housing", None), CategoryId::new());

        assert_eq!(dto.icon, None);
    }

    #[test]
    fn a_valid_id_parses_and_a_malformed_one_is_rejected() {
        let id = CategoryId::new();

        assert_eq!(parse_id(&id.as_string()).unwrap(), id);
        assert!(parse_id("not-a-uuid").is_err());
        assert!(parse_id("").is_err());
    }

    /// `None` means "no parent" (a top-level category), which must not be
    /// conflated with a malformed id — promoting a category to root and
    /// failing to parse its parent are different outcomes.
    #[test]
    fn an_absent_optional_id_is_none_rather_than_an_error() {
        assert_eq!(parse_optional_id(None).unwrap(), None);
        assert!(parse_optional_id(Some("not-a-uuid".to_string())).is_err());
    }
}
