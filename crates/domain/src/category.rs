use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CategoryError {
    #[error("category name cannot be empty")]
    EmptyName,
    #[error("category name cannot be longer than {0} characters")]
    NameTooLong(usize),
    #[error("invalid id: {0}")]
    InvalidId(String),
    #[error("a category cannot be its own parent")]
    SelfParent,
}

const MAX_NAME_LEN: usize = 100;

/// The category new transactions fall back to when none is explicitly
/// chosen (e.g. a CSV import left without a "category for all rows"). Also
/// guaranteed to always exist wherever categories are listed, so it's never
/// missing from a category picker.
pub const DEFAULT_CATEGORY_NAME: &str = "Other";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CategoryId(Uuid);

impl CategoryId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(raw: &str) -> Result<Self, CategoryError> {
        Uuid::parse_str(raw)
            .map(Self)
            .map_err(|_| CategoryError::InvalidId(raw.to_string()))
    }

    pub fn as_string(&self) -> String {
        self.0.to_string()
    }
}

impl Default for CategoryId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryName(String);

impl CategoryName {
    pub fn new(raw: &str) -> Result<Self, CategoryError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(CategoryError::EmptyName);
        }
        if trimmed.chars().count() > MAX_NAME_LEN {
            return Err(CategoryError::NameTooLong(MAX_NAME_LEN));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    id: CategoryId,
    name: CategoryName,
    parent_id: Option<CategoryId>,
}

impl Category {
    pub fn new(
        id: CategoryId,
        name: CategoryName,
        parent_id: Option<CategoryId>,
    ) -> Result<Self, CategoryError> {
        if parent_id == Some(id) {
            return Err(CategoryError::SelfParent);
        }
        Ok(Self {
            id,
            name,
            parent_id,
        })
    }

    pub fn id(&self) -> CategoryId {
        self.id
    }

    pub fn name(&self) -> &CategoryName {
        &self.name
    }

    pub fn parent_id(&self) -> Option<CategoryId> {
        self.parent_id
    }

    pub fn rename(&mut self, name: CategoryName) {
        self.name = name;
    }

    pub fn set_parent(&mut self, parent_id: Option<CategoryId>) -> Result<(), CategoryError> {
        if parent_id == Some(self.id) {
            return Err(CategoryError::SelfParent);
        }
        self.parent_id = parent_id;
        Ok(())
    }
}

/// Domain rule: a subcategory cannot itself have subcategories, so a
/// category that already has children of its own cannot become a
/// subcategory. Needs the whole category list, so this can't live on the
/// `Category` entity itself.
pub fn has_children(id: CategoryId, all: &[Category]) -> bool {
    all.iter().any(|c| c.parent_id() == Some(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn category(id: CategoryId, parent_id: Option<CategoryId>) -> Category {
        Category::new(id, CategoryName::new("Category").unwrap(), parent_id).unwrap()
    }

    #[test]
    fn category_rejects_self_as_parent() {
        let id = CategoryId::new();
        let result = Category::new(id, CategoryName::new("Loop").unwrap(), Some(id));
        assert_eq!(result, Err(CategoryError::SelfParent));
    }

    #[test]
    fn set_parent_rejects_self_as_parent() {
        let id = CategoryId::new();
        let mut cat = category(id, None);

        let result = cat.set_parent(Some(id));

        assert_eq!(result, Err(CategoryError::SelfParent));
    }

    #[test]
    fn has_children_detects_existing_child() {
        let root = CategoryId::new();
        let child = CategoryId::new();
        let all = vec![category(root, None), category(child, Some(root))];

        assert!(has_children(root, &all));
    }

    #[test]
    fn has_children_false_for_leaf_category() {
        let root = CategoryId::new();
        let child = CategoryId::new();
        let all = vec![category(root, None), category(child, Some(root))];

        assert!(!has_children(child, &all));
    }
}
