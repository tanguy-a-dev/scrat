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
    #[error("'{0}' is not a known category icon")]
    UnknownIcon(String),
    #[error("a subcategory cannot have an icon")]
    SubcategoryCannotHaveIcon,
}

const MAX_NAME_LEN: usize = 100;

/// The category new transactions fall back to when none is explicitly
/// chosen (e.g. a CSV import left without a "category for all rows"). Also
/// guaranteed to always exist wherever categories are listed, so it's never
/// missing from a category picker. Matches the seeded "Uncategorized"
/// top-level category (see `scrat_infra_sqlite::seed`) so the fallback
/// reuses it instead of creating a separate duplicate. This is now the one
/// and only default category — forced, not user-selectable — so it can
/// never be renamed or deleted; see `CategoryService::rename_category` and
/// `CategoryService::delete_category`.
pub const DEFAULT_CATEGORY_NAME: &str = "Uncategorized";

/// Closed set of icon identifiers a top-level category can carry: plain
/// kebab-case keys matching lucide's icon names, rather than the domain
/// layer depending on any specific icon library — the frontend owns the
/// key -> rendered-icon mapping.
pub const CATEGORY_ICONS: &[&str] = &[
    "house",
    "shopping-cart",
    "utensils",
    "plug",
    "car",
    "heart-pulse",
    "sparkles",
    "shirt",
    "film",
    "dumbbell",
    "graduation-cap",
    "plane",
    "gift",
    "landmark",
    "receipt",
    "shield",
    "circle-question-mark",
    "briefcase",
    "award",
    "laptop",
    "trending-up",
    "building",
    "rotate-ccw",
    "arrow-left-right",
    "tag",
];

/// Assigned to a newly created top-level category before the user picks
/// something more specific via the icon editor.
pub const FALLBACK_ICON: &str = "tag";

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
pub struct CategoryIcon(String);

impl CategoryIcon {
    pub fn new(raw: &str) -> Result<Self, CategoryError> {
        if !CATEGORY_ICONS.contains(&raw) {
            return Err(CategoryError::UnknownIcon(raw.to_string()));
        }
        Ok(Self(raw.to_string()))
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
    icon: Option<CategoryIcon>,
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
            icon: None,
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

    pub fn icon(&self) -> Option<&CategoryIcon> {
        self.icon.as_ref()
    }

    pub fn rename(&mut self, name: CategoryName) {
        self.name = name;
    }

    pub fn set_parent(&mut self, parent_id: Option<CategoryId>) -> Result<(), CategoryError> {
        if parent_id == Some(self.id) {
            return Err(CategoryError::SelfParent);
        }
        if parent_id.is_some() {
            // Becoming a subcategory — icons are a top-level-only concept.
            self.icon = None;
        }
        self.parent_id = parent_id;
        Ok(())
    }

    /// Only a top-level category may carry an icon — a subcategory renders
    /// inside its parent's card, so an icon on it would never be shown.
    pub fn set_icon(&mut self, icon: Option<CategoryIcon>) -> Result<(), CategoryError> {
        if icon.is_some() && self.parent_id.is_some() {
            return Err(CategoryError::SubcategoryCannotHaveIcon);
        }
        self.icon = icon;
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

    #[test]
    fn category_icon_rejects_unknown_key() {
        assert_eq!(
            CategoryIcon::new("not-a-real-icon"),
            Err(CategoryError::UnknownIcon("not-a-real-icon".to_string()))
        );
    }

    #[test]
    fn category_icon_accepts_a_known_key() {
        assert!(CategoryIcon::new("house").is_ok());
    }

    #[test]
    fn set_icon_rejects_icon_on_a_subcategory() {
        let mut child = category(CategoryId::new(), Some(CategoryId::new()));

        let result = child.set_icon(Some(CategoryIcon::new("house").unwrap()));

        assert_eq!(result, Err(CategoryError::SubcategoryCannotHaveIcon));
    }

    #[test]
    fn set_icon_allows_icon_on_a_root_category() {
        let mut root = category(CategoryId::new(), None);

        let result = root.set_icon(Some(CategoryIcon::new("house").unwrap()));

        assert!(result.is_ok());
        assert_eq!(root.icon().map(CategoryIcon::as_str), Some("house"));
    }

    #[test]
    fn set_parent_clears_icon_when_becoming_a_subcategory() {
        let mut root = category(CategoryId::new(), None);
        root.set_icon(Some(CategoryIcon::new("house").unwrap()))
            .unwrap();

        root.set_parent(Some(CategoryId::new())).unwrap();

        assert_eq!(root.icon(), None);
    }
}
