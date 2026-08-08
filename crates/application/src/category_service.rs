use scrat_domain::category::{
    Category, CategoryError, CategoryIcon, CategoryId, CategoryName, CategorySeedKey,
    DEFAULT_CATEGORY_NAME, FALLBACK_ICON, has_subcategories,
};
use scrat_domain::default_categories::{UNCATEGORIZED_KEY, seeded_name};
use scrat_domain::language::Language;
use scrat_domain::ports::{CategoryRepository, RepositoryError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Category(#[from] CategoryError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("category not found")]
    CategoryNotFound,
    #[error("a subcategory cannot itself be used as a parent")]
    ParentIsSubcategory,
    #[error("category has its own subcategories and cannot become one")]
    HasSubcategories,
    #[error("category still has {0} transaction(s); choose a category to reassign them to")]
    RequiresReassignment(u64),
    #[error("the default '{0}' category cannot be renamed or deleted")]
    DefaultCategoryProtected(String),
}

/// Constructed fresh per request against a live repository borrow — see
/// `AccountService` for why this borrows rather than owns its repository.
pub struct CategoryService<'a> {
    repo: &'a dyn CategoryRepository,
}

impl<'a> CategoryService<'a> {
    pub fn new(repo: &'a dyn CategoryRepository) -> Self {
        Self { repo }
    }

    pub fn create_category(
        &self,
        name: &str,
        parent_id: Option<CategoryId>,
    ) -> Result<Category, ApplicationError> {
        let name = CategoryName::new(name)?;
        if let Some(parent_id) = parent_id {
            let parent = self.get(parent_id)?;
            if parent.parent_id().is_some() {
                return Err(ApplicationError::ParentIsSubcategory);
            }
        }
        let mut category = Category::new(CategoryId::new(), name, parent_id)?;
        if parent_id.is_none() {
            // Every top-level category gets a starting icon the user can
            // then change — see `set_category_icon`.
            category.set_icon(Some(
                CategoryIcon::new(FALLBACK_ICON).expect("fallback icon key is valid"),
            ))?;
        }
        self.repo.insert(&category)?;
        Ok(category)
    }

    pub fn rename_category(&self, id: CategoryId, new_name: &str) -> Result<(), ApplicationError> {
        self.ensure_not_protected(id)?;
        let mut category = self.get(id)?;
        category.rename(CategoryName::new(new_name)?);
        self.repo.update(&category)?;
        Ok(())
    }

    /// Changes a top-level category's icon. Never gated by
    /// `ensure_not_protected` — the forced default category can't be renamed
    /// or deleted, but its icon is still just a display preference.
    pub fn set_category_icon(&self, id: CategoryId, icon: &str) -> Result<(), ApplicationError> {
        let mut category = self.get(id)?;
        category.set_icon(Some(CategoryIcon::new(icon)?))?;
        self.repo.update(&category)?;
        Ok(())
    }

    pub fn move_category(
        &self,
        id: CategoryId,
        new_parent_id: Option<CategoryId>,
    ) -> Result<(), ApplicationError> {
        let mut category = self.get(id)?;
        if let Some(parent_id) = new_parent_id {
            let parent = self.get(parent_id)?;
            if parent.parent_id().is_some() {
                return Err(ApplicationError::ParentIsSubcategory);
            }
            let all = self.repo.list_all()?;
            if has_subcategories(id, &all) {
                return Err(ApplicationError::HasSubcategories);
            }
        }
        category.set_parent(new_parent_id)?;
        self.repo.update(&category)?;
        Ok(())
    }

    /// Deletes the category. Any child categories are re-parented to
    /// `reassign_to` (`None` promotes them to root level). If the category
    /// still has transactions, `reassign_to` must be `Some` — a
    /// transaction's category can never be null.
    pub fn delete_category(
        &self,
        id: CategoryId,
        reassign_to: Option<CategoryId>,
    ) -> Result<(), ApplicationError> {
        self.ensure_not_protected(id)?;
        let transaction_count = self.repo.transaction_count(id)?;
        if transaction_count > 0 {
            let target =
                reassign_to.ok_or(ApplicationError::RequiresReassignment(transaction_count))?;
            if target == id {
                return Err(CategoryError::SelfParent.into());
            }
            self.get(target)?;
            self.repo.reassign_transactions(id, target)?;
        }
        if let Some(target) = reassign_to {
            let all = self.repo.list_all()?;
            if has_subcategories(id, &all) && self.get(target)?.parent_id().is_some() {
                return Err(ApplicationError::ParentIsSubcategory);
            }
        }
        self.repo.reassign_subcategories(id, reassign_to)?;
        self.repo.delete(id)?;
        Ok(())
    }

    pub fn list_categories(&self) -> Result<Vec<Category>, ApplicationError> {
        Ok(self.repo.list_all()?)
    }

    /// Renames every seeded category that still carries the name the app gave
    /// it in `from`, to the name it has in `to`. Returns how many were
    /// renamed.
    ///
    /// The name comparison is the entire policy, and it is deliberately
    /// exact. A category whose name is still character-for-character what the
    /// app wrote is one the user has never expressed an opinion about, so
    /// relabelling it is completing the language change rather than
    /// overwriting a choice. Anything else — renamed, re-cased, or
    /// user-created (no seed key at all) — is theirs, and a language switch
    /// must not touch it. Getting this backwards would silently destroy
    /// user-chosen names, which is unrecoverable; leaving a stale name is
    /// merely untidy, and the user can fix it in one edit.
    ///
    /// Keys this build doesn't recognise (a database written by a newer
    /// version) are skipped rather than cleared, and so are keys whose name is
    /// the same in both languages — "Restaurant" needs no write.
    pub fn relabel_seeded_categories(
        &self,
        from: Language,
        to: Language,
    ) -> Result<usize, ApplicationError> {
        if from == to {
            return Ok(0);
        }
        let mut relabelled = 0;
        for mut category in self.repo.list_all()? {
            let Some(key) = category.seed_key().map(|k| k.as_str().to_string()) else {
                continue;
            };
            let (Some(previous), Some(next)) = (seeded_name(&key, from), seeded_name(&key, to))
            else {
                continue;
            };
            if next == previous || category.name().as_str() != previous {
                continue;
            }
            category.rename(CategoryName::new(next)?);
            self.repo.update(&category)?;
            relabelled += 1;
        }
        Ok(relabelled)
    }

    /// Finds the app's one forced default category, creating it in `language`
    /// if this is the first time anything has needed a fallback — resolved
    /// fresh on every call rather than cached.
    ///
    /// `language` is only consulted on the create path. It has to be, because
    /// a fallback conjured into a French database has to be called `Non
    /// classé`; but a database that already has one keeps whatever it has,
    /// including a name a language change gave it.
    pub fn get_or_create_default_category(
        &self,
        language: Language,
    ) -> Result<Category, ApplicationError> {
        if let Some(existing) = self.find_default_category()? {
            return Ok(existing);
        }
        let name = seeded_name(UNCATEGORIZED_KEY, language).unwrap_or(DEFAULT_CATEGORY_NAME);
        let mut category = self.create_category(name, None)?;
        // Stamped so the next lookup finds it by key, and so a later language
        // change relabels it like any other seeded category.
        category.set_seed_key(Some(CategorySeedKey::new(UNCATEGORIZED_KEY)?));
        self.repo.update(&category)?;
        Ok(category)
    }

    /// The forced default category, if this database has one.
    ///
    /// Identified by seed key first: the category is translatable, so its name
    /// is no longer evidence of what it is. The name check behind it is for
    /// databases whose fallback was created before the key existed *and* whose
    /// migration backfill didn't reach it — without it, such a database would
    /// grow a second "Uncategorized" the first time something needed one.
    fn find_default_category(&self) -> Result<Option<Category>, ApplicationError> {
        let all = self.repo.list_all()?;
        if let Some(found) = all
            .iter()
            .find(|c| c.seed_key().map(CategorySeedKey::as_str) == Some(UNCATEGORIZED_KEY))
        {
            return Ok(Some(found.clone()));
        }
        Ok(all.into_iter().find(|c| {
            c.name()
                .as_str()
                .eq_ignore_ascii_case(DEFAULT_CATEGORY_NAME)
        }))
    }

    /// Refuses the operation if `id` is the forced default category — it's
    /// the bucket transactions fall back to app-wide, so renaming or
    /// deleting it (even via reassignment) is never allowed.
    ///
    /// Looks the category up without creating it: if no fallback exists yet,
    /// then `id` cannot be it, and conjuring one just to compare ids would
    /// need a language this call has no business knowing.
    fn ensure_not_protected(&self, id: CategoryId) -> Result<(), ApplicationError> {
        if self.find_default_category()?.map(|c| c.id()) == Some(id) {
            return Err(ApplicationError::DefaultCategoryProtected(
                DEFAULT_CATEGORY_NAME.to_string(),
            ));
        }
        Ok(())
    }

    fn get(&self, id: CategoryId) -> Result<Category, ApplicationError> {
        self.repo
            .find_by_id(id)?
            .ok_or(ApplicationError::CategoryNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeCategoryRepository {
        categories: Mutex<Vec<Category>>,
        transaction_counts: Mutex<std::collections::HashMap<CategoryId, u64>>,
    }

    impl CategoryRepository for FakeCategoryRepository {
        fn insert(&self, category: &Category) -> Result<(), RepositoryError> {
            self.categories.lock().unwrap().push(category.clone());
            Ok(())
        }

        fn update(&self, category: &Category) -> Result<(), RepositoryError> {
            let mut categories = self.categories.lock().unwrap();
            let existing = categories
                .iter_mut()
                .find(|c| c.id() == category.id())
                .expect("category must exist");
            *existing = category.clone();
            Ok(())
        }

        fn delete(&self, id: CategoryId) -> Result<(), RepositoryError> {
            self.categories.lock().unwrap().retain(|c| c.id() != id);
            Ok(())
        }

        fn find_by_id(&self, id: CategoryId) -> Result<Option<Category>, RepositoryError> {
            Ok(self
                .categories
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.id() == id)
                .cloned())
        }

        fn list_all(&self) -> Result<Vec<Category>, RepositoryError> {
            Ok(self.categories.lock().unwrap().clone())
        }

        fn reassign_subcategories(
            &self,
            from: CategoryId,
            to: Option<CategoryId>,
        ) -> Result<(), RepositoryError> {
            for category in self.categories.lock().unwrap().iter_mut() {
                if category.parent_id() == Some(from) {
                    category.set_parent(to).ok();
                }
            }
            Ok(())
        }

        fn reassign_transactions(
            &self,
            from: CategoryId,
            to: CategoryId,
        ) -> Result<(), RepositoryError> {
            let mut counts = self.transaction_counts.lock().unwrap();
            let moved = counts.remove(&from).unwrap_or(0);
            *counts.entry(to).or_insert(0) += moved;
            Ok(())
        }

        fn transaction_count(&self, id: CategoryId) -> Result<u64, RepositoryError> {
            Ok(*self
                .transaction_counts
                .lock()
                .unwrap()
                .get(&id)
                .unwrap_or(&0))
        }
    }

    #[test]
    fn get_or_create_default_category_creates_it_when_missing() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);

        let category = service
            .get_or_create_default_category(Language::En)
            .unwrap();

        assert_eq!(category.name().as_str(), DEFAULT_CATEGORY_NAME);
    }

    /// A fallback conjured into a French database has to read as French. The
    /// stamped key is what lets the *next* language change find it again.
    #[test]
    fn get_or_create_default_category_creates_it_in_the_given_language() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);

        let category = service
            .get_or_create_default_category(Language::Fr)
            .unwrap();

        assert_eq!(category.name().as_str(), "Non classé");
        assert_eq!(
            category.seed_key().map(CategorySeedKey::as_str),
            Some(UNCATEGORIZED_KEY)
        );
    }

    /// The fallback is identified by key, not by name — otherwise a French
    /// database would fail to recognise its own `Non classé` and grow a
    /// second one the first time anything needed a default.
    #[test]
    fn a_relabelled_fallback_is_still_found_by_its_key() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let existing = service
            .get_or_create_default_category(Language::Fr)
            .unwrap();

        let found = service
            .get_or_create_default_category(Language::Fr)
            .unwrap();

        assert_eq!(found.id(), existing.id());
        assert_eq!(repo.categories.lock().unwrap().len(), 1);
    }

    /// And it stays protected once relabelled. Keying the protection off the
    /// English name would have let a French user delete the one category the
    /// whole app falls back to.
    #[test]
    fn a_relabelled_fallback_still_cannot_be_renamed_or_deleted() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let fallback = service
            .get_or_create_default_category(Language::Fr)
            .unwrap();

        assert!(matches!(
            service.rename_category(fallback.id(), "Divers"),
            Err(ApplicationError::DefaultCategoryProtected(_))
        ));
        assert!(matches!(
            service.delete_category(fallback.id(), None),
            Err(ApplicationError::DefaultCategoryProtected(_))
        ));
    }

    fn seeded(
        service: &CategoryService,
        repo: &FakeCategoryRepository,
        key: &str,
        name: &str,
    ) -> CategoryId {
        let category = service.create_category(name, None).unwrap();
        let mut stored = category.clone();
        stored.set_seed_key(Some(CategorySeedKey::new(key).unwrap()));
        repo.update(&stored).unwrap();
        category.id()
    }

    fn name_of(repo: &FakeCategoryRepository, id: CategoryId) -> String {
        repo.find_by_id(id)
            .unwrap()
            .unwrap()
            .name()
            .as_str()
            .to_string()
    }

    #[test]
    fn relabelling_renames_seeded_categories_that_still_have_their_seeded_name() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let housing = seeded(&service, &repo, "housing", "Housing");

        let count = service
            .relabel_seeded_categories(Language::En, Language::Fr)
            .unwrap();

        assert_eq!(count, 1);
        assert_eq!(name_of(&repo, housing), "Logement");
    }

    /// The line the whole feature turns on. A name the user chose is theirs;
    /// a language switch relabels the app, not their data.
    #[test]
    fn relabelling_never_touches_a_category_the_user_renamed() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let renamed = seeded(&service, &repo, "housing", "Our flat");
        let user_made = service.create_category("Boat fuel", None).unwrap();

        let count = service
            .relabel_seeded_categories(Language::En, Language::Fr)
            .unwrap();

        assert_eq!(count, 0);
        assert_eq!(name_of(&repo, renamed), "Our flat");
        assert_eq!(name_of(&repo, user_made.id()), "Boat fuel");
    }

    /// Switching back has to restore the English names, not strand the
    /// database in French — the relabel has to work in both directions.
    #[test]
    fn relabelling_round_trips_between_languages() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let housing = seeded(&service, &repo, "housing", "Housing");

        service
            .relabel_seeded_categories(Language::En, Language::Fr)
            .unwrap();
        service
            .relabel_seeded_categories(Language::Fr, Language::En)
            .unwrap();

        assert_eq!(name_of(&repo, housing), "Housing");
    }

    /// Names that are the same word in both languages cost no write at all —
    /// and, more importantly, are not reported as changes the user made.
    #[test]
    fn relabelling_skips_categories_whose_name_is_identical_in_both_languages() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        seeded(&service, &repo, "food_and_drink.restaurant", "Restaurant");

        let count = service
            .relabel_seeded_categories(Language::En, Language::Fr)
            .unwrap();

        assert_eq!(count, 0);
    }

    /// A key written by a newer build is data this one doesn't understand.
    /// Leaving the name alone is the only safe answer — clearing or guessing
    /// would corrupt a category the user can see.
    #[test]
    fn relabelling_leaves_an_unknown_seed_key_alone() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let unknown = seeded(&service, &repo, "crypto.staking_rewards", "Staking Rewards");

        let count = service
            .relabel_seeded_categories(Language::En, Language::Fr)
            .unwrap();

        assert_eq!(count, 0);
        assert_eq!(name_of(&repo, unknown), "Staking Rewards");
    }

    #[test]
    fn relabelling_to_the_same_language_is_a_no_op() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        seeded(&service, &repo, "housing", "Housing");

        let count = service
            .relabel_seeded_categories(Language::En, Language::En)
            .unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn get_or_create_default_category_reuses_existing_one_case_insensitively() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let existing = service.create_category("uncategorized", None).unwrap();

        let category = service
            .get_or_create_default_category(Language::En)
            .unwrap();

        assert_eq!(category.id(), existing.id());
        assert_eq!(repo.categories.lock().unwrap().len(), 1);
    }

    #[test]
    fn rename_category_rejects_the_default_category() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let default_category = service
            .get_or_create_default_category(Language::En)
            .unwrap();

        let result = service.rename_category(default_category.id(), "Renamed");

        assert!(matches!(
            result,
            Err(ApplicationError::DefaultCategoryProtected(_))
        ));
    }

    #[test]
    fn delete_category_rejects_the_default_category() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let default_category = service
            .get_or_create_default_category(Language::En)
            .unwrap();

        let result = service.delete_category(default_category.id(), None);

        assert!(matches!(
            result,
            Err(ApplicationError::DefaultCategoryProtected(_))
        ));
    }

    #[test]
    fn create_category_with_valid_name_succeeds() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);

        let category = service.create_category("Hobby", None).unwrap();

        assert_eq!(category.name().as_str(), "Hobby");
        assert_eq!(category.parent_id(), None);
    }

    #[test]
    fn create_category_assigns_a_fallback_icon_to_root_categories() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);

        let category = service.create_category("Hobby", None).unwrap();

        assert_eq!(category.icon().map(CategoryIcon::as_str), Some("tag"));
    }

    #[test]
    fn create_category_leaves_subcategories_without_an_icon() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let hobby = service.create_category("Hobby", None).unwrap();

        let paint = service.create_category("Paint", Some(hobby.id())).unwrap();

        assert_eq!(paint.icon(), None);
    }

    #[test]
    fn set_category_icon_updates_a_root_categorys_icon() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let hobby = service.create_category("Hobby", None).unwrap();

        service.set_category_icon(hobby.id(), "house").unwrap();

        let stored = repo.find_by_id(hobby.id()).unwrap().unwrap();
        assert_eq!(stored.icon().map(CategoryIcon::as_str), Some("house"));
    }

    #[test]
    fn set_category_icon_rejects_an_unknown_icon_key() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let hobby = service.create_category("Hobby", None).unwrap();

        let result = service.set_category_icon(hobby.id(), "not-a-real-icon");

        assert!(matches!(result, Err(ApplicationError::Category(_))));
    }

    #[test]
    fn set_category_icon_rejects_a_subcategory() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let hobby = service.create_category("Hobby", None).unwrap();
        let paint = service.create_category("Paint", Some(hobby.id())).unwrap();

        let result = service.set_category_icon(paint.id(), "house");

        assert!(matches!(result, Err(ApplicationError::Category(_))));
    }

    #[test]
    fn set_category_icon_is_allowed_on_the_protected_default_category() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let default_category = service
            .get_or_create_default_category(Language::En)
            .unwrap();

        let result = service.set_category_icon(default_category.id(), "house");

        assert!(result.is_ok());
    }

    #[test]
    fn create_category_with_unknown_parent_fails() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);

        let result = service.create_category("Paint", Some(CategoryId::new()));

        assert!(matches!(result, Err(ApplicationError::CategoryNotFound)));
    }

    #[test]
    fn create_category_with_known_parent_succeeds() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let hobby = service.create_category("Hobby", None).unwrap();

        let paint = service.create_category("Paint", Some(hobby.id())).unwrap();

        assert_eq!(paint.parent_id(), Some(hobby.id()));
    }

    #[test]
    fn create_category_rejects_subcategory_as_parent() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let hobby = service.create_category("Hobby", None).unwrap();
        let paint = service.create_category("Paint", Some(hobby.id())).unwrap();

        let result = service.create_category("Watercolor", Some(paint.id()));

        assert!(matches!(result, Err(ApplicationError::ParentIsSubcategory)));
    }

    #[test]
    fn move_category_rejects_subcategory_as_parent() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let hobby = service.create_category("Hobby", None).unwrap();
        let paint = service.create_category("Paint", Some(hobby.id())).unwrap();
        let other = service.create_category("Other", None).unwrap();

        let result = service.move_category(other.id(), Some(paint.id()));

        assert!(matches!(result, Err(ApplicationError::ParentIsSubcategory)));
    }

    #[test]
    fn move_category_rejects_when_category_has_subcategories() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let hobby = service.create_category("Hobby", None).unwrap();
        service.create_category("Paint", Some(hobby.id())).unwrap();
        let other = service.create_category("Other", None).unwrap();

        let result = service.move_category(hobby.id(), Some(other.id()));

        assert!(matches!(result, Err(ApplicationError::HasSubcategories)));
    }

    #[test]
    fn move_category_allows_valid_reparent() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let a = service.create_category("A", None).unwrap();
        let b = service.create_category("B", None).unwrap();

        service.move_category(b.id(), Some(a.id())).unwrap();

        let stored = repo.find_by_id(b.id()).unwrap().unwrap();
        assert_eq!(stored.parent_id(), Some(a.id()));
    }

    #[test]
    fn delete_category_without_subcategories_or_transactions_succeeds() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let category = service.create_category("Temp", None).unwrap();

        service.delete_category(category.id(), None).unwrap();

        assert!(repo.find_by_id(category.id()).unwrap().is_none());
    }

    #[test]
    fn delete_category_rejects_reassigning_subcategories_to_a_subcategory() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let hobby = service.create_category("Hobby", None).unwrap();
        service.create_category("Paint", Some(hobby.id())).unwrap();
        let sports = service.create_category("Sports", None).unwrap();
        let football = service
            .create_category("Football", Some(sports.id()))
            .unwrap();

        let result = service.delete_category(hobby.id(), Some(football.id()));

        assert!(matches!(result, Err(ApplicationError::ParentIsSubcategory)));
    }

    #[test]
    fn delete_category_reassigns_subcategories_to_given_target() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let hobby = service.create_category("Hobby", None).unwrap();
        let paint = service.create_category("Paint", Some(hobby.id())).unwrap();
        let other = service.create_category("Other", None).unwrap();

        service
            .delete_category(hobby.id(), Some(other.id()))
            .unwrap();

        let stored = repo.find_by_id(paint.id()).unwrap().unwrap();
        assert_eq!(stored.parent_id(), Some(other.id()));
    }

    #[test]
    fn delete_category_with_transactions_requires_reassignment_target() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let category = service.create_category("Groceries", None).unwrap();
        repo.transaction_counts
            .lock()
            .unwrap()
            .insert(category.id(), 5);

        let result = service.delete_category(category.id(), None);

        assert!(matches!(
            result,
            Err(ApplicationError::RequiresReassignment(5))
        ));
    }

    #[test]
    fn delete_category_with_transactions_reassigns_when_target_given() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let category = service.create_category("Groceries", None).unwrap();
        let other = service.create_category("Food", None).unwrap();
        repo.transaction_counts
            .lock()
            .unwrap()
            .insert(category.id(), 5);

        service
            .delete_category(category.id(), Some(other.id()))
            .unwrap();

        assert_eq!(
            *repo
                .transaction_counts
                .lock()
                .unwrap()
                .get(&other.id())
                .unwrap(),
            5
        );
    }
}
