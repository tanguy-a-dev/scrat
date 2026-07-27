use scrat_application::category_service::CategoryService;
use scrat_domain::category::{Category, CategoryId};
use scrat_infra_sqlite::SqliteCategoryRepository;
use serde::Serialize;
use tauri::State;

use crate::db::DbState;

#[derive(Debug, Serialize)]
pub struct CategoryDto {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
}

impl From<Category> for CategoryDto {
    fn from(category: Category) -> Self {
        Self {
            id: category.id().as_string(),
            name: category.name().as_str().to_string(),
            parent_id: category.parent_id().map(|p| p.as_string()),
        }
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

#[tauri::command]
pub fn list_categories(state: State<DbState>) -> Result<Vec<CategoryDto>, String> {
    with_service(&state, |s| s.list_categories())
        .map(|categories| categories.into_iter().map(CategoryDto::from).collect())
}

#[tauri::command]
pub fn create_category(
    state: State<DbState>,
    name: String,
    parent_id: Option<String>,
) -> Result<CategoryDto, String> {
    let parent_id = parse_optional_id(parent_id)?;
    with_service(&state, |s| s.create_category(&name, parent_id)).map(CategoryDto::from)
}

#[tauri::command]
pub fn rename_category(state: State<DbState>, id: String, name: String) -> Result<(), String> {
    let id = parse_id(&id)?;
    with_service(&state, |s| s.rename_category(id, &name))
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
