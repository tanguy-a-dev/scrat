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
    #[error("a category seed key cannot be empty")]
    EmptySeedKey,
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
///
/// This is the *English* name, and since the interface became translatable
/// it is no longer how the fallback is identified — that is
/// [`crate::default_categories::UNCATEGORIZED_KEY`], which survives the
/// category being relabelled to `Non classé`. The name remains as the
/// spelling used when creating the fallback from scratch, and as a
/// last-resort match for databases predating the seed-key column.
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

/// Marks a category as one the app itself created from its built-in default
/// set, and says *which* of them it is. See
/// [`crate::default_categories`] for the catalogue these key into and why the
/// key exists at all.
///
/// Open rather than a closed enum on purpose: a database written by a newer
/// build can carry keys this one has never heard of, and refusing to load
/// that row would turn a forward-compatible file into an unopenable one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategorySeedKey(String);

impl CategorySeedKey {
    pub fn new(raw: &str) -> Result<Self, CategoryError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(CategoryError::EmptySeedKey);
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
    icon: Option<CategoryIcon>,
    /// `Some` only for categories the app seeded. User-created categories
    /// have `None` and are never touched by a language change.
    seed_key: Option<CategorySeedKey>,
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
            seed_key: None,
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

    pub fn seed_key(&self) -> Option<&CategorySeedKey> {
        self.seed_key.as_ref()
    }

    /// Set once, when the app seeds the category (or when a repository
    /// rehydrates a stored row). Deliberately *not* cleared by [`rename`]:
    /// which built-in category a row is stays true even after the user
    /// renames it, and it is exactly what lets a later language change tell
    /// "still called what we named it" from "the user has made this theirs".
    ///
    /// [`rename`]: Category::rename
    pub fn set_seed_key(&mut self, seed_key: Option<CategorySeedKey>) {
        self.seed_key = seed_key;
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
/// category that already has subcategories of its own cannot become a
/// subcategory. Needs the whole category list, so this can't live on the
/// `Category` entity itself.
pub fn has_subcategories(id: CategoryId, all: &[Category]) -> bool {
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
    fn has_subcategories_detects_existing_subcategory() {
        let root = CategoryId::new();
        let child = CategoryId::new();
        let all = vec![category(root, None), category(child, Some(root))];

        assert!(has_subcategories(root, &all));
    }

    #[test]
    fn has_subcategories_false_for_leaf_category() {
        let root = CategoryId::new();
        let child = CategoryId::new();
        let all = vec![category(root, None), category(child, Some(root))];

        assert!(!has_subcategories(child, &all));
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
    fn a_category_is_not_seeded_by_default() {
        assert_eq!(category(CategoryId::new(), None).seed_key(), None);
    }

    /// The distinction the whole relabel feature rests on: renaming a seeded
    /// category must not make the app forget which built-in it was. Clearing
    /// the key on rename would mean a category could never be relabelled back
    /// after a round trip through the user's own wording.
    #[test]
    fn renaming_a_seeded_category_keeps_its_seed_key() {
        let mut cat = category(CategoryId::new(), None);
        cat.set_seed_key(Some(CategorySeedKey::new("housing").unwrap()));

        cat.rename(CategoryName::new("Chez moi").unwrap());

        assert_eq!(cat.seed_key().map(CategorySeedKey::as_str), Some("housing"));
        assert_eq!(cat.name().as_str(), "Chez moi");
    }

    #[test]
    fn a_seed_key_cannot_be_empty() {
        assert_eq!(
            CategorySeedKey::new("   "),
            Err(CategoryError::EmptySeedKey)
        );
        assert_eq!(CategorySeedKey::new(""), Err(CategoryError::EmptySeedKey));
    }

    /// A key from a newer build must load, not fail — the column is
    /// forward-compatible storage, not a closed vocabulary.
    #[test]
    fn a_seed_key_accepts_a_key_this_build_does_not_know() {
        let key = CategorySeedKey::new("crypto.staking_rewards").unwrap();
        assert_eq!(key.as_str(), "crypto.staking_rewards");
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
