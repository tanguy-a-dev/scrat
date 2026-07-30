use scrat_domain::category::{
    has_children, Category, CategoryError, CategoryId, CategoryName, DEFAULT_CATEGORY_NAME,
};
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
        let category = Category::new(CategoryId::new(), name, parent_id)?;
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
            if has_children(id, &all) {
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
            if has_children(id, &all) && self.get(target)?.parent_id().is_some() {
                return Err(ApplicationError::ParentIsSubcategory);
            }
        }
        self.repo.reassign_children(id, reassign_to)?;
        self.repo.delete(id)?;
        Ok(())
    }

    pub fn list_categories(&self) -> Result<Vec<Category>, ApplicationError> {
        Ok(self.repo.list_all()?)
    }

    /// Finds the category named "Uncategorized" (case-insensitive), creating
    /// it if this is the first time anything has needed a fallback default —
    /// the app's one forced default category, resolved fresh on every call
    /// rather than cached, since it can never be renamed away from this name
    /// (see `ensure_not_protected`).
    pub fn get_or_create_default_category(&self) -> Result<Category, ApplicationError> {
        if let Some(existing) = self.repo.list_all()?.into_iter().find(|c| {
            c.name()
                .as_str()
                .eq_ignore_ascii_case(DEFAULT_CATEGORY_NAME)
        }) {
            return Ok(existing);
        }
        self.create_category(DEFAULT_CATEGORY_NAME, None)
    }

    /// Refuses the operation if `id` is the forced default category — it's
    /// the bucket transactions fall back to app-wide, so renaming or
    /// deleting it (even via reassignment) is never allowed.
    fn ensure_not_protected(&self, id: CategoryId) -> Result<(), ApplicationError> {
        if self.get_or_create_default_category()?.id() == id {
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

        fn reassign_children(
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

        let category = service.get_or_create_default_category().unwrap();

        assert_eq!(category.name().as_str(), DEFAULT_CATEGORY_NAME);
    }

    #[test]
    fn get_or_create_default_category_reuses_existing_one_case_insensitively() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let existing = service.create_category("uncategorized", None).unwrap();

        let category = service.get_or_create_default_category().unwrap();

        assert_eq!(category.id(), existing.id());
        assert_eq!(repo.categories.lock().unwrap().len(), 1);
    }

    #[test]
    fn rename_category_rejects_the_default_category() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let default_category = service.get_or_create_default_category().unwrap();

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
        let default_category = service.get_or_create_default_category().unwrap();

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
    fn move_category_rejects_when_category_has_children() {
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
    fn delete_category_without_children_or_transactions_succeeds() {
        let repo = FakeCategoryRepository::default();
        let service = CategoryService::new(&repo);
        let category = service.create_category("Temp", None).unwrap();

        service.delete_category(category.id(), None).unwrap();

        assert!(repo.find_by_id(category.id()).unwrap().is_none());
    }

    #[test]
    fn delete_category_rejects_reassigning_children_to_a_subcategory() {
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
    fn delete_category_reassigns_children_to_given_target() {
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
