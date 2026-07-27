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

/// Domain service: would re-parenting `target` under `candidate_parent`
/// create a cycle? Needs the whole category graph, so this can't live on
/// the `Category` entity itself.
pub fn would_create_cycle(
    target: CategoryId,
    candidate_parent: CategoryId,
    all: &[Category],
) -> bool {
    let mut current = Some(candidate_parent);
    while let Some(id) = current {
        if id == target {
            return true;
        }
        current = all
            .iter()
            .find(|c| c.id() == id)
            .and_then(|c| c.parent_id());
    }
    false
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
    fn would_create_cycle_detects_direct_cycle() {
        let root = CategoryId::new();
        let child = CategoryId::new();
        let all = vec![category(root, None), category(child, Some(root))];

        // Trying to make root's parent be its own child.
        assert!(would_create_cycle(root, child, &all));
    }

    #[test]
    fn would_create_cycle_detects_indirect_cycle_through_grandchild() {
        let root = CategoryId::new();
        let child = CategoryId::new();
        let grandchild = CategoryId::new();
        let all = vec![
            category(root, None),
            category(child, Some(root)),
            category(grandchild, Some(child)),
        ];

        // root -> child -> grandchild; making root a child of grandchild loops.
        assert!(would_create_cycle(root, grandchild, &all));
    }

    #[test]
    fn would_create_cycle_allows_moving_to_unrelated_branch() {
        let root = CategoryId::new();
        let branch_a = CategoryId::new();
        let branch_b = CategoryId::new();
        let all = vec![
            category(root, None),
            category(branch_a, Some(root)),
            category(branch_b, Some(root)),
        ];

        assert!(!would_create_cycle(branch_a, branch_b, &all));
    }

    #[test]
    fn would_create_cycle_allows_reassigning_same_parent() {
        let root = CategoryId::new();
        let child = CategoryId::new();
        let all = vec![category(root, None), category(child, Some(root))];

        assert!(!would_create_cycle(child, root, &all));
    }
}
