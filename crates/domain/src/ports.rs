use chrono::NaiveDate;
use thiserror::Error;

use crate::account::{Account, AccountId};
use crate::category::{Category, CategoryId};
use crate::transaction::{OperationKind, Transaction, TransactionId, TransferGroupId};
use crate::transfer_rule::{TransferRule, TransferRuleId};

#[derive(Debug, Error)]
#[error("{0}")]
pub struct RepositoryError(pub String);

/// Port for persisting accounts. Implemented by an infra-* adapter (SQLite
/// in production, an in-memory fake in application-layer tests) — the
/// application layer depends only on this trait, never on a concrete
/// storage technology.
pub trait AccountRepository {
    fn insert(&self, account: &Account) -> Result<(), RepositoryError>;
    fn update(&self, account: &Account) -> Result<(), RepositoryError>;
    fn delete(&self, id: AccountId) -> Result<(), RepositoryError>;
    fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError>;
    fn list_all(&self) -> Result<Vec<Account>, RepositoryError>;
    /// Count of transactions referencing this account — used to decide
    /// whether a delete must fall back to archiving instead.
    fn transaction_count(&self, id: AccountId) -> Result<u64, RepositoryError>;
    /// Sum of all transaction amounts (minor units) posted to this account.
    /// Zero when there are none.
    fn sum_transactions_minor_units(&self, id: AccountId) -> Result<i64, RepositoryError>;
}

/// Port for persisting categories (a self-referential tree via `parent_id`).
pub trait CategoryRepository {
    fn insert(&self, category: &Category) -> Result<(), RepositoryError>;
    fn update(&self, category: &Category) -> Result<(), RepositoryError>;
    fn delete(&self, id: CategoryId) -> Result<(), RepositoryError>;
    fn find_by_id(&self, id: CategoryId) -> Result<Option<Category>, RepositoryError>;
    fn list_all(&self) -> Result<Vec<Category>, RepositoryError>;
    /// Re-parents every category whose `parent_id` is `from` to `to`
    /// (`None` promotes them to root level). A no-op when there are none.
    fn reassign_subcategories(
        &self,
        from: CategoryId,
        to: Option<CategoryId>,
    ) -> Result<(), RepositoryError>;
    /// Moves every transaction referencing `from` to `to`. `to` must be a
    /// real category — a transaction's category can never be null.
    fn reassign_transactions(
        &self,
        from: CategoryId,
        to: CategoryId,
    ) -> Result<(), RepositoryError>;
    /// Count of transactions referencing this category — used to decide
    /// whether a delete requires an explicit reassignment target.
    fn transaction_count(&self, id: CategoryId) -> Result<u64, RepositoryError>;
}

/// The filters `list_page` and `count_in_range` both take — bundled rather
/// than passed as an ever-growing list of positional `Option`s, since a page
/// and the header count over it must apply exactly the same predicate to
/// agree on what "matching" means. All fields are `None`/empty by default,
/// meaning "no filter", not "match nothing".
#[derive(Debug, Clone, Default)]
pub struct TransactionFilters {
    /// Matches the named category **and its subcategories**, not the named
    /// category alone. Naming a parent has to mean the whole branch: that is
    /// what every total in the app already reports (the Details donut rolls
    /// children into their root), so an exact-match filter would answer a
    /// question nobody asked — "rows filed directly against Housing" — with
    /// a number contradicting the one the user clicked to get here. Naming a
    /// leaf is unaffected: the two-level hierarchy guarantees a subcategory
    /// has no children of its own, so the branch is just itself.
    pub category_id: Option<CategoryId>,
    /// Case-insensitive substring match.
    pub description_contains: Option<String>,
    /// `true` narrows to positive amounts, `false` to negative, `None` to
    /// both — lets the Expenses and Income lists page through the ledger
    /// independently, each with its own filters.
    pub is_income: Option<bool>,
    pub account_id: Option<AccountId>,
    pub operation_kind: Option<OperationKind>,
    /// Inclusive bounds on the transaction's *unsigned* amount. Expenses and
    /// income are already split by `is_income`, so "amount between X and Y"
    /// means magnitude, not the signed minor units.
    pub min_amount_minor_units: Option<i64>,
    pub max_amount_minor_units: Option<i64>,
}

/// What `list_page` orders a paginated walk by. `Category` and `Account`
/// sort by the linked aggregate's display name, not its id — an adapter with
/// access to those names (e.g. via a SQL join) must resolve them, since an id
/// order would be meaningless to the person reading the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransactionSortField {
    #[default]
    Date,
    Amount,
    Description,
    OperationKind,
    Category,
    Account,
}

impl TransactionSortField {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Amount => "amount",
            Self::Description => "description",
            Self::OperationKind => "operation_kind",
            Self::Category => "category",
            Self::Account => "account",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, RepositoryError> {
        match raw {
            "date" => Ok(Self::Date),
            "amount" => Ok(Self::Amount),
            "description" => Ok(Self::Description),
            "operation_kind" => Ok(Self::OperationKind),
            "category" => Ok(Self::Category),
            "account" => Ok(Self::Account),
            other => Err(RepositoryError(format!("unknown sort field: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    Asc,
    #[default]
    Desc,
}

impl SortDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, RepositoryError> {
        match raw {
            "asc" => Ok(Self::Asc),
            "desc" => Ok(Self::Desc),
            other => Err(RepositoryError(format!("unknown sort direction: {other}"))),
        }
    }
}

/// Port for persisting transactions (the ledger).
pub trait TransactionRepository {
    /// Inserts the transaction. Two transactions with the same (account,
    /// date, amount, description) are both allowed — that's a legitimate
    /// coincidence (e.g. two identical coffees the same day), not a
    /// duplicate to reject. Re-importing the same CSV twice is the caller's
    /// responsibility to avoid; this port does no deduplication.
    fn insert(&self, transaction: &Transaction) -> Result<(), RepositoryError>;
    fn delete(&self, id: TransactionId) -> Result<(), RepositoryError>;
    fn find_by_id(&self, id: TransactionId) -> Result<Option<Transaction>, RepositoryError>;
    /// Deletes both legs of a transfer at once. Deleting only the leg the
    /// user is looking at would leave the counterpart account permanently
    /// overstated by the transfer amount, with nothing on screen to explain
    /// the gap — so removal is all-or-nothing, same as creation.
    fn delete_transfer_group(&self, group_id: TransferGroupId) -> Result<(), RepositoryError>;
    /// Recategorizes an existing transaction — used when the user corrects
    /// or updates a transaction's category after the fact.
    fn update_category(
        &self,
        id: TransactionId,
        category_id: CategoryId,
    ) -> Result<(), RepositoryError>;
    /// Deletes every listed transaction in one statement. Ids that don't
    /// exist are ignored rather than an error — the caller (bulk delete)
    /// expands transfer legs to their whole group and de-duplicates, so the
    /// same row can legitimately be named twice. Implementations must apply
    /// this atomically: a partial delete would leave the caller unable to
    /// tell the user what actually happened.
    fn delete_many(&self, ids: &[TransactionId]) -> Result<(), RepositoryError>;
    /// Recategorizes every listed transaction in one statement, atomically.
    fn update_category_many(
        &self,
        ids: &[TransactionId],
        category_id: CategoryId,
    ) -> Result<(), RepositoryError>;
    fn list_in_range(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Transaction>, RepositoryError>;
    /// Every transaction, unfiltered. Not `list_in_range(NaiveDate::MIN,
    /// NaiveDate::MAX)` — those extremes format with a leading sign
    /// (`+262142-12-31`) that sorts before ordinary years in a TEXT
    /// comparison, so that combination silently matches nothing.
    fn list_all(&self) -> Result<Vec<Transaction>, RepositoryError>;
    /// Newest-first, `limit` transactions starting at `offset` — lets a
    /// caller walk the whole ledger in fixed-size batches instead of paying
    /// for `list_all` up front. Batch size is a count, not a calendar
    /// window, deliberately: a year of history can be one row or one
    /// hundred thousand depending on the user, so only a row count keeps
    /// each batch cheap.
    ///
    /// `filters` are the same ones `count_in_range` takes, and they are
    /// pushed down here for the same reason: a filtered paginated view must
    /// page through *matching* rows. Filtering the returned batch in the
    /// caller instead would only ever surface the matches that happen to
    /// fall inside the pages already fetched, so a filter would appear to
    /// find almost nothing until the user scrolled the entire ledger in.
    ///
    /// `sort_field`/`sort_dir` are pushed down for the identical reason:
    /// sorting the returned batch in the caller only reorders whatever pages
    /// happen to be loaded already, which is not the same as "sorted by X
    /// across the whole matching set" — a batch is a page of rows already in
    /// the requested order, wherever in the ledger they live.
    fn list_page(
        &self,
        offset: i64,
        limit: i64,
        filters: &TransactionFilters,
        sort_field: TransactionSortField,
        sort_dir: SortDirection,
    ) -> Result<Vec<Transaction>, RepositoryError>;
    /// Counts transactions in `[start, end]`, narrowed by `filters` — the
    /// same ones `list_page` takes. Pushed down to the query (rather than
    /// counting a `list_in_range` result in the caller) so a paginated view
    /// can show an accurate total for the current filters without first
    /// pulling every matching row across the wire.
    fn count_in_range(
        &self,
        start: NaiveDate,
        end: NaiveDate,
        filters: &TransactionFilters,
    ) -> Result<i64, RepositoryError>;
}

/// Port for persisting the rules that recognize an imported row as a
/// transfer to another of the user's own accounts.
pub trait TransferRuleRepository {
    fn insert(&self, rule: &TransferRule) -> Result<(), RepositoryError>;
    fn delete(&self, id: TransferRuleId) -> Result<(), RepositoryError>;
    fn list_all(&self) -> Result<Vec<TransferRule>, RepositoryError>;
}
