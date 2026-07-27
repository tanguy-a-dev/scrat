use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use scrat_domain::category::{Category, CategoryId, CategoryName};
use scrat_domain::ports::{CategoryRepository, RepositoryError};

pub struct SqliteCategoryRepository<'a> {
    conn: &'a Connection,
}

impl<'a> SqliteCategoryRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    fn row_to_category(row: &rusqlite::Row) -> rusqlite::Result<Category> {
        let id_str: String = row.get("id")?;
        let id = CategoryId::parse(&id_str).map_err(|e| {
            rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
        })?;
        let name: String = row.get("name")?;
        let name = CategoryName::new(&name).map_err(|e| {
            rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
        })?;
        let parent_id: Option<String> = row.get("parent_id")?;
        let parent_id = parent_id
            .map(|p| CategoryId::parse(&p))
            .transpose()
            .map_err(|e| {
                rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
            })?;

        Category::new(id, name, parent_id).map_err(|e| {
            rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
        })
    }
}

fn sql_err(e: rusqlite::Error) -> RepositoryError {
    RepositoryError(e.to_string())
}

impl<'a> CategoryRepository for SqliteCategoryRepository<'a> {
    fn insert(&self, category: &Category) -> Result<(), RepositoryError> {
        self.conn
            .execute(
                "INSERT INTO categories (id, name, parent_id, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    category.id().as_string(),
                    category.name().as_str(),
                    category.parent_id().map(|p| p.as_string()),
                    Utc::now().to_rfc3339(),
                ],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn update(&self, category: &Category) -> Result<(), RepositoryError> {
        self.conn
            .execute(
                "UPDATE categories SET name = ?2, parent_id = ?3 WHERE id = ?1",
                params![
                    category.id().as_string(),
                    category.name().as_str(),
                    category.parent_id().map(|p| p.as_string()),
                ],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn delete(&self, id: CategoryId) -> Result<(), RepositoryError> {
        self.conn
            .execute(
                "DELETE FROM categories WHERE id = ?1",
                params![id.as_string()],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn find_by_id(&self, id: CategoryId) -> Result<Option<Category>, RepositoryError> {
        self.conn
            .query_row(
                "SELECT id, name, parent_id FROM categories WHERE id = ?1",
                params![id.as_string()],
                Self::row_to_category,
            )
            .optional()
            .map_err(sql_err)
    }

    fn list_all(&self) -> Result<Vec<Category>, RepositoryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, parent_id FROM categories ORDER BY name")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], Self::row_to_category)
            .map_err(sql_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        Ok(rows)
    }

    fn reassign_children(
        &self,
        from: CategoryId,
        to: Option<CategoryId>,
    ) -> Result<(), RepositoryError> {
        self.conn
            .execute(
                "UPDATE categories SET parent_id = ?2 WHERE parent_id = ?1",
                params![from.as_string(), to.map(|t| t.as_string())],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn reassign_transactions(
        &self,
        from: CategoryId,
        to: CategoryId,
    ) -> Result<(), RepositoryError> {
        self.conn
            .execute(
                "UPDATE transactions SET category_id = ?2 WHERE category_id = ?1",
                params![from.as_string(), to.as_string()],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn transaction_count(&self, id: CategoryId) -> Result<u64, RepositoryError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE category_id = ?1",
                params![id.as_string()],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(sql_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("scrat.db");
        crate::create_new(&path, "test passphrase").unwrap()
    }

    #[test]
    fn persists_and_reloads_roundtrip() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let category =
            Category::new(CategoryId::new(), CategoryName::new("Hobby").unwrap(), None).unwrap();

        repo.insert(&category).unwrap();
        let reloaded = repo.find_by_id(category.id()).unwrap().unwrap();

        assert_eq!(reloaded.name().as_str(), "Hobby");
        assert_eq!(reloaded.parent_id(), None);
    }

    #[test]
    fn persists_parent_child_relationship() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let hobby =
            Category::new(CategoryId::new(), CategoryName::new("Hobby").unwrap(), None).unwrap();
        repo.insert(&hobby).unwrap();
        let paint = Category::new(
            CategoryId::new(),
            CategoryName::new("Paint").unwrap(),
            Some(hobby.id()),
        )
        .unwrap();
        repo.insert(&paint).unwrap();

        let reloaded = repo.find_by_id(paint.id()).unwrap().unwrap();

        assert_eq!(reloaded.parent_id(), Some(hobby.id()));
    }

    #[test]
    fn reassign_children_moves_them_to_new_parent() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let hobby =
            Category::new(CategoryId::new(), CategoryName::new("Hobby").unwrap(), None).unwrap();
        repo.insert(&hobby).unwrap();
        let paint = Category::new(
            CategoryId::new(),
            CategoryName::new("Paint").unwrap(),
            Some(hobby.id()),
        )
        .unwrap();
        repo.insert(&paint).unwrap();
        let other =
            Category::new(CategoryId::new(), CategoryName::new("Other").unwrap(), None).unwrap();
        repo.insert(&other).unwrap();

        repo.reassign_children(hobby.id(), Some(other.id()))
            .unwrap();

        let reloaded = repo.find_by_id(paint.id()).unwrap().unwrap();
        assert_eq!(reloaded.parent_id(), Some(other.id()));
    }

    #[test]
    fn transaction_count_reflects_inserted_rows() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let category = Category::new(
            CategoryId::new(),
            CategoryName::new("Groceries").unwrap(),
            None,
        )
        .unwrap();
        repo.insert(&category).unwrap();

        conn.execute(
            "INSERT INTO accounts (id, name, status, opening_balance_minor_units, created_at, updated_at)
             VALUES ('acc-1', 'Checking', 'active', 0, datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (id, date, amount_minor_units, source, category_id, account_id, dedup_key, created_at)
             VALUES ('tx-1', '2026-01-01', -500, 'Store', ?1, 'acc-1', 'dedup-1', datetime('now'))",
            params![category.id().as_string()],
        )
        .unwrap();

        assert_eq!(repo.transaction_count(category.id()).unwrap(), 1);
    }

    #[test]
    fn reassign_transactions_moves_them_to_new_category() {
        let conn = test_conn();
        let repo = SqliteCategoryRepository::new(&conn);
        let groceries = Category::new(
            CategoryId::new(),
            CategoryName::new("Groceries").unwrap(),
            None,
        )
        .unwrap();
        repo.insert(&groceries).unwrap();
        let food =
            Category::new(CategoryId::new(), CategoryName::new("Food").unwrap(), None).unwrap();
        repo.insert(&food).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, name, status, opening_balance_minor_units, created_at, updated_at)
             VALUES ('acc-1', 'Checking', 'active', 0, datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (id, date, amount_minor_units, source, category_id, account_id, dedup_key, created_at)
             VALUES ('tx-1', '2026-01-01', -500, 'Store', ?1, 'acc-1', 'dedup-1', datetime('now'))",
            params![groceries.id().as_string()],
        )
        .unwrap();

        repo.reassign_transactions(groceries.id(), food.id())
            .unwrap();

        assert_eq!(repo.transaction_count(groceries.id()).unwrap(), 0);
        assert_eq!(repo.transaction_count(food.id()).unwrap(), 1);
    }
}
