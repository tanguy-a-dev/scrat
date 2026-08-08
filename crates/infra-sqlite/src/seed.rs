//! Default category tree created once, when a brand-new database is set up
//! (see `connection::create_new`) — never reapplied on `unlock_existing`, so
//! renaming or deleting a seeded category sticks.
//!
//! The tree itself lives in `scrat_domain::default_categories`, not here:
//! `CategoryService::relabel_seeded_categories` needs the same list to rename
//! untouched categories when the interface language changes, and the
//! application layer cannot depend on this crate. This module is only the
//! part that writes it down.

use rusqlite::Connection;
use scrat_domain::category::{Category, CategoryIcon, CategoryId, CategoryName, CategorySeedKey};
use scrat_domain::default_categories::DEFAULT_CATEGORIES;
use scrat_domain::language::Language;
use scrat_domain::ports::{CategoryRepository, RepositoryError};

use crate::category_repository::SqliteCategoryRepository;

/// Populates a freshly-created database with a curated set of top-level
/// categories and subcategories, so the user isn't staring at an empty
/// category picker on first run. Every name and icon key is a fixed,
/// known-valid literal from the domain catalogue, so constructor failures are
/// treated as a programmer error (`expect`), not a runtime condition callers
/// need to handle — `default_categories`' own tests are what keep that true.
///
/// Each row carries its `seed_key`, which is what lets a later language change
/// find these categories again after it has renamed them.
pub fn seed_default_categories(
    conn: &Connection,
    language: Language,
) -> Result<(), RepositoryError> {
    let repo = SqliteCategoryRepository::new(conn);
    for entry in DEFAULT_CATEGORIES {
        let mut parent = Category::new(
            CategoryId::new(),
            CategoryName::new(entry.name(language)).expect("seed category name is valid"),
            None,
        )
        .expect("seed top-level category has no parent");
        parent
            .set_icon(Some(
                CategoryIcon::new(entry.icon).expect("seed icon key is valid"),
            ))
            .expect("top-level seed category can carry an icon");
        parent.set_seed_key(Some(
            CategorySeedKey::new(entry.key).expect("seed key is non-empty"),
        ));
        repo.insert(&parent)?;

        for child in entry.children {
            let mut subcategory = Category::new(
                CategoryId::new(),
                CategoryName::new(child.name(language)).expect("seed category name is valid"),
                Some(parent.id()),
            )
            .expect("seed child cannot be its own parent");
            subcategory.set_seed_key(Some(
                CategorySeedKey::new(child.key).expect("seed key is non-empty"),
            ));
            repo.insert(&subcategory)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use scrat_domain::default_categories::UNCATEGORIZED_KEY;

    use super::*;

    fn test_conn() -> Connection {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("scrat.db");
        crate::create_new(&path, "test passphrase").unwrap()
    }

    /// Counted from the seed list rather than written out: this assertion
    /// is about every seeded category reaching the database, not about the
    /// curated list being any particular length. Hardcoding the total meant
    /// that curating the list — the whole point of it being a list — broke
    /// an unrelated-looking test.
    fn seeded_category_count() -> usize {
        DEFAULT_CATEGORIES
            .iter()
            .map(|entry| 1 + entry.children.len())
            .sum()
    }

    #[test]
    fn seeding_happens_automatically_on_create_new() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);

        let all = repo.list_all().unwrap();

        assert_eq!(all.len(), seeded_category_count());
        assert!(all.len() > 1, "the seed list should not be empty");
    }

    /// A new database starts in the app's default language. The setting that
    /// would say otherwise lives in this same database and cannot exist
    /// before it does, so there is nothing else it could be — switching
    /// afterwards is what `relabel_seeded_categories` is for.
    #[test]
    fn create_new_seeds_in_the_default_language() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let all = repo.list_all().unwrap();

        assert!(all.iter().any(|c| c.name().as_str() == "Housing"));
        assert!(!all.iter().any(|c| c.name().as_str() == "Logement"));
    }

    #[test]
    fn seeded_subcategory_points_at_its_seeded_parent() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let all = repo.list_all().unwrap();

        let housing = all.iter().find(|c| c.name().as_str() == "Housing").unwrap();
        let rent = all.iter().find(|c| c.name().as_str() == "Rent").unwrap();

        assert_eq!(housing.parent_id(), None);
        assert_eq!(rent.parent_id(), Some(housing.id()));
    }

    #[test]
    fn seeded_categories_respect_the_two_level_hierarchy() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let all = repo.list_all().unwrap();

        for category in all.iter().filter(|c| c.parent_id().is_some()) {
            assert!(
                !scrat_domain::category::has_subcategories(category.id(), &all),
                "'{}' is a subcategory with children of its own",
                category.name().as_str()
            );
        }
    }

    #[test]
    fn seeded_top_level_categories_have_an_icon() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let all = repo.list_all().unwrap();

        let housing = all.iter().find(|c| c.name().as_str() == "Housing").unwrap();

        assert_eq!(housing.icon().map(CategoryIcon::as_str), Some("house"));
    }

    #[test]
    fn seeded_subcategories_have_no_icon() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let all = repo.list_all().unwrap();

        let rent = all.iter().find(|c| c.name().as_str() == "Rent").unwrap();

        assert_eq!(rent.icon(), None);
    }

    /// Without a key surviving the round trip through SQLite, a language
    /// change has no way to find these rows again — the column is the
    /// feature, not bookkeeping.
    #[test]
    fn every_seeded_category_stores_its_key() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let all = repo.list_all().unwrap();

        assert!(
            all.iter().all(|c| c.seed_key().is_some()),
            "every seeded category should carry a seed key"
        );
        let housing = all.iter().find(|c| c.name().as_str() == "Housing").unwrap();
        assert_eq!(
            housing.seed_key().map(CategorySeedKey::as_str),
            Some("housing")
        );
        let uncategorized = all
            .iter()
            .find(|c| c.seed_key().map(CategorySeedKey::as_str) == Some(UNCATEGORIZED_KEY))
            .expect("the fallback category is seeded");
        assert_eq!(uncategorized.name().as_str(), "Uncategorized");
    }

    /// Renaming goes through `update`, which writes every column including
    /// the key. A statement that dropped it would quietly un-seed the
    /// category the first time anyone renamed it — including the app's own
    /// relabel pass, which renames every seeded category at once.
    #[test]
    fn renaming_a_seeded_category_preserves_its_key_through_the_repository() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let all = repo.list_all().unwrap();
        let mut housing = all
            .into_iter()
            .find(|c| c.name().as_str() == "Housing")
            .unwrap();

        housing.rename(CategoryName::new("Logement").unwrap());
        repo.update(&housing).unwrap();

        let reloaded = repo.find_by_id(housing.id()).unwrap().unwrap();
        assert_eq!(reloaded.name().as_str(), "Logement");
        assert_eq!(
            reloaded.seed_key().map(CategorySeedKey::as_str),
            Some("housing")
        );
    }

    /// The French tree has to be insertable too — a name that trips
    /// `CategoryName`'s validation, or a missing translation, would panic
    /// through `expect` on a real user's language change rather than here.
    #[test]
    fn the_french_tree_seeds_completely() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("french.db");
        let conn = crate::create_new(&path, "test passphrase").unwrap();
        // Subcategories first: `parent_id` is a foreign key onto this same
        // table, so deleting a parent while its children still point at it
        // fails the constraint.
        conn.execute("DELETE FROM categories WHERE parent_id IS NOT NULL", [])
            .unwrap();
        conn.execute("DELETE FROM categories", []).unwrap();

        seed_default_categories(&conn, Language::Fr).unwrap();

        let repo = SqliteCategoryRepository::new(&conn);
        let all = repo.list_all().unwrap();
        assert_eq!(all.len(), seeded_category_count());
        assert!(all.iter().any(|c| c.name().as_str() == "Logement"));
        assert!(all.iter().any(|c| c.name().as_str() == "Non classé"));
    }
}
