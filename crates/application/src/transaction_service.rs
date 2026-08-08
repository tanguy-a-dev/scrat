use chrono::NaiveDate;
use scrat_domain::account::AccountId;
use scrat_domain::category::{Category, CategoryError, CategoryId, CategoryName};
use scrat_domain::money::{Currency, Money};
use scrat_domain::ports::{
    AccountRepository, CategoryRepository, RepositoryError, SortDirection, TransactionFilters,
    TransactionRepository, TransactionSortField,
};
use scrat_domain::recurring::{self, RecurringCharge};
use scrat_domain::transaction::{
    Description, OperationKind, Transaction, TransactionError, TransactionFingerprint,
    TransactionId, TransactionRole, TransferGroupId,
};
use scrat_domain::transfer_rule::TransferRule;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Transaction(#[from] TransactionError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Category(#[from] CategoryError),
    #[error("account not found")]
    AccountNotFound,
    #[error("category not found")]
    CategoryNotFound,
    #[error("balance is too large to reconcile against")]
    BalanceOutOfRange,
}

/// A single row a CSV importer has already parsed into a date/amount/description
/// triple, with the category it should be filed under already resolved.
/// Deliberately independent of any particular CSV crate's types —
/// `scrat-infra-csv` produces its own parsed rows and the Tauri command
/// layer maps them into this before calling [`TransactionService::import_transactions`].
#[derive(Debug, Clone)]
pub struct ImportRow {
    pub date: NaiveDate,
    pub amount_minor_units: i64,
    pub description: String,
    pub category_id: CategoryId,
    /// How the money moved, as the importer read it off the file. Purely
    /// descriptive — it never decides whether a row becomes a transfer pair
    /// below. That is a transfer *rule* naming another of the user's own
    /// accounts, and nothing else: a row labeled `BankTransfer` with no
    /// matching rule is a wire to someone else, which is ordinary spending.
    pub operation_kind: OperationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportOutcome {
    /// Rows taken from the file. A transfer row counts once here even
    /// though it writes two ledger entries — the user chose one row.
    pub imported: usize,
    /// How many of those rows a transfer rule recognized, and so also
    /// produced a mirrored leg on a counterpart account.
    pub mirrored: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetroactiveTransferOutcome {
    pub converted: usize,
}

/// Result of a bulk delete. `deleted` can exceed the number of ids the
/// caller passed in — deleting one leg of a transfer deletes its
/// counterpart too, even if that counterpart was never in the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BulkDeleteOutcome {
    pub deleted: usize,
    /// How many transfer groups were touched — lets the caller say "N
    /// transfers removed on both accounts" instead of just a row count.
    pub transfer_groups: usize,
}

/// The description text recorded on a reconciliation adjustment. Fixed rather
/// than user-supplied so these entries are recognizable at a glance in the
/// ledger, next to the imported rows they sit among.
pub const RECONCILIATION_DESCRIPTION: &str = "Balance adjustment";

/// Constructed fresh per request against live repository borrows — see
/// `AccountService` for why these borrow rather than own their repository.
pub struct TransactionService<'a> {
    transactions: &'a dyn TransactionRepository,
    accounts: &'a dyn AccountRepository,
    categories: &'a dyn CategoryRepository,
    currency: Currency,
}

impl<'a> TransactionService<'a> {
    pub fn new(
        transactions: &'a dyn TransactionRepository,
        accounts: &'a dyn AccountRepository,
        categories: &'a dyn CategoryRepository,
        currency: Currency,
    ) -> Self {
        Self {
            transactions,
            accounts,
            categories,
            currency,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_transaction(
        &self,
        date: NaiveDate,
        amount_minor_units: i64,
        description: &str,
        category_id: CategoryId,
        account_id: AccountId,
    ) -> Result<Transaction, ApplicationError> {
        self.categories
            .find_by_id(category_id)?
            .ok_or(ApplicationError::CategoryNotFound)?;
        self.accounts
            .find_by_id(account_id)?
            .ok_or(ApplicationError::AccountNotFound)?;

        let description = Description::new(description)?;
        let amount = Money::from_minor_units(amount_minor_units, self.currency.clone());
        let transaction = Transaction::new(
            TransactionId::new(),
            date,
            amount,
            description,
            category_id,
            account_id,
        )?;
        self.transactions.insert(&transaction)?;
        Ok(transaction)
    }

    /// Deletes a transaction — and, when it is one leg of a transfer, its
    /// counterpart too. Removing a single leg would leave the other account
    /// permanently overstated by the transfer amount, and nothing on that
    /// account's screen would explain where the money came from.
    pub fn delete_transaction(&self, id: TransactionId) -> Result<(), ApplicationError> {
        match self
            .transactions
            .find_by_id(id)?
            .and_then(|t| t.transfer_group_id())
        {
            Some(group_id) => self.transactions.delete_transfer_group(group_id)?,
            None => self.transactions.delete(id)?,
        }
        Ok(())
    }

    /// Recategorizes an existing transaction, e.g. after the user notices it
    /// was filed under the wrong category.
    pub fn set_category(
        &self,
        id: TransactionId,
        category_id: CategoryId,
    ) -> Result<(), ApplicationError> {
        self.categories
            .find_by_id(category_id)?
            .ok_or(ApplicationError::CategoryNotFound)?;
        self.transactions.update_category(id, category_id)?;
        Ok(())
    }

    /// Deletes every listed transaction, expanding each id that belongs to a
    /// transfer to its whole group — same rule as `delete_transaction`, so a
    /// bulk selection can never remove one leg and silently overstate the
    /// counterpart account. A selection containing both legs of the same
    /// transfer still counts and deletes that group once.
    ///
    /// `deleted` in the result can exceed `ids.len()`: deleting one leg
    /// always brings its counterpart with it, even if that counterpart was
    /// never part of the selection.
    pub fn delete_transactions(
        &self,
        ids: &[TransactionId],
    ) -> Result<BulkDeleteOutcome, ApplicationError> {
        let mut standalone = Vec::new();
        let mut group_ids: std::collections::HashSet<TransferGroupId> =
            std::collections::HashSet::new();
        for &id in ids {
            match self
                .transactions
                .find_by_id(id)?
                .and_then(|t| t.transfer_group_id())
            {
                Some(group_id) => {
                    group_ids.insert(group_id);
                }
                None => standalone.push(id),
            }
        }
        self.transactions.delete_many(&standalone)?;
        for group_id in &group_ids {
            self.transactions.delete_transfer_group(*group_id)?;
        }
        Ok(BulkDeleteOutcome {
            deleted: standalone.len() + group_ids.len() * 2,
            transfer_groups: group_ids.len(),
        })
    }

    /// Recategorizes every listed transaction in one batch. The category is
    /// validated once up front, not once per id — same check as
    /// `set_category`, just not repeated N times.
    pub fn set_category_many(
        &self,
        ids: &[TransactionId],
        category_id: CategoryId,
    ) -> Result<(), ApplicationError> {
        self.categories
            .find_by_id(category_id)?
            .ok_or(ApplicationError::CategoryNotFound)?;
        self.transactions.update_category_many(ids, category_id)?;
        Ok(())
    }

    pub fn list_in_range(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Transaction>, ApplicationError> {
        Ok(self.transactions.list_in_range(start, end)?)
    }

    pub fn list_all(&self) -> Result<Vec<Transaction>, ApplicationError> {
        Ok(self.transactions.list_all()?)
    }

    /// Every transaction on one account, in `list_all`'s order.
    ///
    /// A use-case rather than a filter at the call site, because it verifies
    /// the account exists first: naming an account that isn't there is a
    /// programming error the caller should hear about, not something to
    /// report as "this account has no transactions" by writing an empty
    /// export file.
    ///
    /// Note this returns *every* role, including the mirrored leg of a
    /// transfer whose origin was another account, and any `Adjustment` the
    /// account has been reconciled with. Those rows are as much a part of
    /// this account's ledger as any other — an account whose export omitted
    /// them wouldn't sum to the balance the app shows for it.
    pub fn list_for_account(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<Transaction>, ApplicationError> {
        self.accounts
            .find_by_id(account_id)?
            .ok_or(ApplicationError::AccountNotFound)?;
        Ok(self
            .transactions
            .list_all()?
            .into_iter()
            .filter(|t| t.account_id() == account_id)
            .collect())
    }

    pub fn list_page(
        &self,
        offset: i64,
        limit: i64,
        filters: &TransactionFilters,
        sort_field: TransactionSortField,
        sort_dir: SortDirection,
    ) -> Result<Vec<Transaction>, ApplicationError> {
        Ok(self
            .transactions
            .list_page(offset, limit, filters, sort_field, sort_dir)?)
    }

    pub fn count_in_range(
        &self,
        start: NaiveDate,
        end: NaiveDate,
        filters: &TransactionFilters,
    ) -> Result<i64, ApplicationError> {
        Ok(self.transactions.count_in_range(start, end, filters)?)
    }

    /// Scans `[start, today]` for recurring commitments — subscriptions, rent,
    /// utilities. Nothing is stored: the result is derived from the ledger on
    /// every call, so cancelling a service and re-importing is all it takes to
    /// keep it honest.
    ///
    /// The lookback is the caller's to choose because it is a genuine
    /// trade-off rather than a detail. Too short and a monthly charge can't
    /// reach the three occurrences detection needs; too long and something
    /// cancelled long ago keeps reappearing (flagged lapsed, but still there).
    pub fn detect_recurring_charges(
        &self,
        start: NaiveDate,
        today: NaiveDate,
    ) -> Result<Vec<RecurringCharge>, ApplicationError> {
        let transactions = self.transactions.list_in_range(start, today)?;
        Ok(recurring::detect_recurring_charges(&transactions, today))
    }

    /// Imports a batch of already-parsed rows (from a CSV, say) into the
    /// given account, each under its own already-resolved category.
    ///
    /// This is *not* idempotent: re-importing a file, or an overlapping date
    /// range, writes the rows again. Identical (account, date, amount,
    /// description) transactions are legitimate — two identical coffees the same
    /// day — so there's no safe automatic rule for telling a real repeat
    /// from a re-import, and the caller is left to avoid it.
    ///
    /// A row whose description text matches a `transfer_rules` entry is money
    /// moving to another of the user's own accounts, not spending. Those
    /// rows are written as a pair: the outflow here, and a mirrored inflow
    /// on the counterpart. This is the only way an account whose statements
    /// can't be exported gets its incoming side of the ledger at all — and
    /// because the pair is created and deleted together, a duplicated import
    /// duplicates both legs symmetrically, so cleaning up the origin account
    /// cleans up the counterpart too.
    pub fn import_transactions(
        &self,
        rows: &[ImportRow],
        account_id: AccountId,
        transfer_rules: &[TransferRule],
    ) -> Result<ImportOutcome, ApplicationError> {
        self.accounts
            .find_by_id(account_id)?
            .ok_or(ApplicationError::AccountNotFound)?;

        let mut outcome = ImportOutcome::default();
        for row in rows {
            self.categories
                .find_by_id(row.category_id)?
                .ok_or(ApplicationError::CategoryNotFound)?;
            let description = Description::new(&row.description)?;
            let amount = Money::from_minor_units(row.amount_minor_units, self.currency.clone());

            // A rule pointing back at the account being imported would
            // mirror the row onto itself, doubling it and netting the
            // account's balance change to zero. Treat it as an ordinary row.
            let counterpart_id = transfer_rules
                .iter()
                .find(|rule| rule.matches_description(&row.description))
                .map(|rule| rule.counterpart_account_id())
                .filter(|id| *id != account_id);

            match counterpart_id {
                Some(counterpart_id) => {
                    self.accounts
                        .find_by_id(counterpart_id)?
                        .ok_or(ApplicationError::AccountNotFound)?;
                    let group_id = TransferGroupId::new();
                    let outflow = Transaction::new_with_role(
                        TransactionId::new(),
                        row.date,
                        amount,
                        description,
                        row.category_id,
                        account_id,
                        TransactionRole::Transfer,
                        Some(group_id),
                    )?
                    .with_operation_kind(row.operation_kind);
                    let inflow = outflow.mirrored_onto(counterpart_id, group_id)?;
                    self.transactions.insert(&outflow)?;
                    self.transactions.insert(&inflow)?;
                    outcome.mirrored += 1;
                }
                None => {
                    let transaction = Transaction::new(
                        TransactionId::new(),
                        row.date,
                        amount,
                        description,
                        row.category_id,
                        account_id,
                    )?
                    .with_operation_kind(row.operation_kind);
                    self.transactions.insert(&transaction)?;
                }
            }
            outcome.imported += 1;
        }
        Ok(outcome)
    }

    /// Brings an account whose statements can't be imported back in line
    /// with the balance the user actually observes, by posting the
    /// difference as a single [`TransactionRole::Adjustment`] entry.
    ///
    /// Returns `None` when the ledger already agrees — reconciling an
    /// account that needs nothing should leave no trace, so repeatedly
    /// checking doesn't litter the ledger with zero-value entries.
    ///
    /// The adjustment deliberately doesn't try to explain *why* the balance
    /// drifted. For an account holding investments the drift is mostly
    /// market movement, and it is not separable from ordinary spending on
    /// the same account without the statements this exists to work around.
    pub fn reconcile_account(
        &self,
        account_id: AccountId,
        observed_balance_minor_units: i64,
        category_id: CategoryId,
        date: NaiveDate,
    ) -> Result<Option<Transaction>, ApplicationError> {
        let account = self
            .accounts
            .find_by_id(account_id)?
            .ok_or(ApplicationError::AccountNotFound)?;
        self.categories
            .find_by_id(category_id)?
            .ok_or(ApplicationError::CategoryNotFound)?;

        let ledger_sum = self.accounts.sum_transactions_minor_units(account_id)?;
        // The observed balance is typed in by hand, so every step here is
        // checked: an extra digit or two shouldn't wrap a balance around.
        let current = account
            .opening_balance_minor_units()
            .checked_add(ledger_sum)
            .ok_or(ApplicationError::BalanceOutOfRange)?;
        let delta = observed_balance_minor_units
            .checked_sub(current)
            .ok_or(ApplicationError::BalanceOutOfRange)?;
        if delta == 0 {
            return Ok(None);
        }

        let adjustment = Transaction::new_with_role(
            TransactionId::new(),
            date,
            Money::from_minor_units(delta, self.currency.clone()),
            Description::new(RECONCILIATION_DESCRIPTION)?,
            category_id,
            account_id,
            TransactionRole::Adjustment,
            None,
        )?;
        self.transactions.insert(&adjustment)?;
        Ok(Some(adjustment))
    }

    /// Catches up transactions already in the ledger to a transfer rule that
    /// didn't exist yet when they were imported — the retroactive companion
    /// to the rule-matching [`Self::import_transactions`] already does for
    /// new rows. Without this, a transaction imported before the rule was
    /// added stays filed as ordinary spending forever, and its counterpart
    /// account never receives the mirrored leg it should have had all along.
    ///
    /// Only [`TransactionRole::Normal`] rows are candidates — an existing
    /// transfer or adjustment is left alone, which is what makes running
    /// this again (after adding another rule, say) safe: it can't
    /// double-convert or double-mirror a row it already touched.
    pub fn apply_transfer_rules_to_existing(
        &self,
        transfer_rules: &[TransferRule],
    ) -> Result<RetroactiveTransferOutcome, ApplicationError> {
        let mut outcome = RetroactiveTransferOutcome::default();
        for transaction in self.transactions.list_all()? {
            if transaction.role() != TransactionRole::Normal {
                continue;
            }
            let Some(rule) = transfer_rules
                .iter()
                .find(|rule| rule.matches_description(transaction.description().as_str()))
            else {
                continue;
            };
            let counterpart_id = rule.counterpart_account_id();
            // Same self-transfer guard as import: a rule pointing back at
            // the row's own account would mirror it onto itself.
            if counterpart_id == transaction.account_id() {
                continue;
            }
            self.accounts
                .find_by_id(counterpart_id)?
                .ok_or(ApplicationError::AccountNotFound)?;

            let group_id = TransferGroupId::new();
            // Reuses the transaction's own id — it's being reclassified in
            // place, not replaced by a new entry.
            let outflow = Transaction::new_with_role(
                transaction.id(),
                transaction.date(),
                transaction.amount().clone(),
                transaction.description().clone(),
                transaction.category_id(),
                transaction.account_id(),
                TransactionRole::Transfer,
                Some(group_id),
            )?;
            let inflow = outflow.mirrored_onto(counterpart_id, group_id)?;

            self.transactions.delete(transaction.id())?;
            self.transactions.insert(&outflow)?;
            self.transactions.insert(&inflow)?;
            outcome.converted += 1;
        }
        Ok(outcome)
    }

    /// Finds the account whose description-pattern list matches the given raw
    /// bank description text, if any — used to suggest (not force) an account
    /// while the user is filling in a transaction's description field.
    pub fn find_account_by_description(
        &self,
        description: &str,
    ) -> Result<Option<AccountId>, ApplicationError> {
        Ok(self
            .accounts
            .list_all()?
            .into_iter()
            .find(|a| a.matches_description(description))
            .map(|a| a.id()))
    }

    /// Finds an existing top-level or subcategory matching `name`
    /// (case-insensitive), or creates a new top-level category for it —
    /// used to honor a CSV's own Category column during import rather than
    /// forcing every row into one chosen category.
    pub fn get_or_create_category_by_name(
        &self,
        name: &str,
    ) -> Result<CategoryId, ApplicationError> {
        let trimmed = name.trim();
        if let Some(existing) = self
            .categories
            .list_all()?
            .into_iter()
            .find(|c| c.name().as_str().eq_ignore_ascii_case(trimmed))
        {
            return Ok(existing.id());
        }
        let category = Category::new(CategoryId::new(), CategoryName::new(trimmed)?, None)?;
        self.categories.insert(&category)?;
        Ok(category.id())
    }

    /// Like [`Self::get_or_create_category_by_name`], but also honors a CSV's
    /// own Subcategory column: `category_name` is resolved (or created) as a
    /// top-level category first, then `subcategory_name`, if given, is
    /// resolved (or created) as *its* child — matching the app's strict
    /// two-level hierarchy. A `category_name` that already exists as a
    /// subcategory elsewhere is not reused as a parent (that would nest a
    /// third level); a new top-level category is created instead.
    pub fn get_or_create_category_path(
        &self,
        category_name: &str,
        subcategory_name: Option<&str>,
    ) -> Result<CategoryId, ApplicationError> {
        let category_name = category_name.trim();
        let subcategory_name = subcategory_name.map(str::trim).filter(|s| !s.is_empty());

        let all = self.categories.list_all()?;
        let parent_id = match all.iter().find(|c| {
            c.parent_id().is_none() && c.name().as_str().eq_ignore_ascii_case(category_name)
        }) {
            Some(existing) => existing.id(),
            None => {
                let category =
                    Category::new(CategoryId::new(), CategoryName::new(category_name)?, None)?;
                self.categories.insert(&category)?;
                category.id()
            }
        };

        let Some(subcategory_name) = subcategory_name else {
            return Ok(parent_id);
        };

        if let Some(existing) = all.iter().find(|c| {
            c.parent_id() == Some(parent_id)
                && c.name().as_str().eq_ignore_ascii_case(subcategory_name)
        }) {
            return Ok(existing.id());
        }
        let subcategory = Category::new(
            CategoryId::new(),
            CategoryName::new(subcategory_name)?,
            Some(parent_id),
        )?;
        self.categories.insert(&subcategory)?;
        Ok(subcategory.id())
    }

    /// Flags which of `rows` collide with a transaction already sitting in
    /// `account_id` — same date, amount, and normalized description. Used by
    /// CSV import to default those rows unticked, via
    /// [`TransactionFingerprint`] — the candidate key it was built for. This
    /// is a hint, not a constraint: nothing stops the caller from importing
    /// a flagged row anyway, the same way [`TransactionRepository::insert`]
    /// never rejects a duplicate fingerprint.
    pub fn find_duplicate_rows(
        &self,
        account_id: AccountId,
        rows: &[(NaiveDate, i64, String)],
    ) -> Result<Vec<bool>, ApplicationError> {
        let existing: std::collections::HashSet<String> = self
            .transactions
            .list_all()?
            .into_iter()
            .filter(|t| t.account_id() == account_id)
            .map(|t| t.fingerprint().as_str().to_string())
            .collect();

        Ok(rows
            .iter()
            .map(|(date, amount_minor_units, description)| {
                let fingerprint =
                    TransactionFingerprint::of(account_id, *date, *amount_minor_units, description);
                existing.contains(fingerprint.as_str())
            })
            .collect())
    }

    /// Finds past transactions whose description text matches `description` exactly
    /// (case-insensitive, whitespace-normalized) and returns the category of
    /// the most recent one by transaction date — used by CSV import to
    /// categorize rows the file itself doesn't specify a category for. Last
    /// transaction speaks the truth: if past transactions with this description
    /// disagree on category, the most recent one wins.
    pub fn find_category_for_description(
        &self,
        description: &str,
    ) -> Result<Option<CategoryId>, ApplicationError> {
        let normalized = normalize_description(description);
        if normalized.is_empty() {
            return Ok(None);
        }

        Ok(self
            .matching_description_transactions(&normalized)?
            .into_iter()
            .max_by_key(|t| t.date())
            .map(|t| t.category_id()))
    }

    /// Like [`Self::find_category_for_description`], but scoped to past
    /// transactions filed under `category_name` (or one of its
    /// subcategories) — used when a CSV row specifies a category but leaves
    /// the subcategory blank, so history can still fill in the specific
    /// subcategory this description is usually filed under without overriding
    /// the category the row itself already pins down. Last transaction
    /// speaks the truth, same as [`Self::find_category_for_description`].
    pub fn find_category_for_description_in_category(
        &self,
        description: &str,
        category_name: &str,
    ) -> Result<Option<CategoryId>, ApplicationError> {
        let normalized = normalize_description(description);
        if normalized.is_empty() {
            return Ok(None);
        }

        let categories = self.categories.list_all()?;
        let is_in_category = |id: CategoryId| -> bool {
            categories.iter().find(|c| c.id() == id).is_some_and(|c| {
                c.name().as_str().eq_ignore_ascii_case(category_name)
                    || c.parent_id().is_some_and(|parent_id| {
                        categories.iter().any(|p| {
                            p.id() == parent_id
                                && p.name().as_str().eq_ignore_ascii_case(category_name)
                        })
                    })
            })
        };

        Ok(self
            .matching_description_transactions(&normalized)?
            .into_iter()
            .filter(|t| is_in_category(t.category_id()))
            .max_by_key(|t| t.date())
            .map(|t| t.category_id()))
    }

    fn matching_description_transactions(
        &self,
        normalized_description: &str,
    ) -> Result<Vec<Transaction>, ApplicationError> {
        Ok(self
            .transactions
            .list_all()?
            .into_iter()
            .filter(|t| normalize_description(t.description().as_str()) == normalized_description)
            .collect())
    }

    /// Local frequency lookup, no ML/network: finds past transactions that
    /// share a significant word with `description`, and suggests whichever
    /// category is most common among them.
    pub fn suggest_category_for_description(
        &self,
        description: &str,
    ) -> Result<Option<CategoryId>, ApplicationError> {
        let query_tokens = tokenize(description);
        if query_tokens.is_empty() {
            return Ok(None);
        }

        let all = self.transactions.list_all()?;

        let mut counts: std::collections::HashMap<CategoryId, usize> =
            std::collections::HashMap::new();
        for t in &all {
            let candidate_tokens = tokenize(t.description().as_str());
            if query_tokens.iter().any(|qt| candidate_tokens.contains(qt)) {
                *counts.entry(t.category_id()).or_insert(0) += 1;
            }
        }

        Ok(counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(id, _)| id))
    }
}

/// Case-insensitive, whitespace-collapsed form of a description string, matching
/// the normalization `TransactionFingerprint::compute` applies before hashing — the
/// convention this repo already uses to decide whether two description texts
/// "are the same" for comparison purposes.
///
/// Also drops a leading "cb" token: some banks prefix every card-payment
/// description with "CB" (Carte Bancaire), which would otherwise make a
/// transaction fail to match its own history the moment the bank starts (or
/// stops) adding that marker. This is narrower than `merchant_key`'s noise
/// filter in `recurring.rs`, which deliberately leaves bank prefixes alone —
/// here we're matching a merchant against its own past self, not merging
/// distinct merchants, so stripping a known, fixed marker is safe.
///
/// Also drops any date-like token (`30/06/26`, `14/02/2026`) anywhere in the
/// text: some banks embed the transaction date inside the description itself
/// (`CB SOME STORE 30/06/26`), so the same merchant would otherwise never
/// match its own history from a different day.
fn normalize_description(s: &str) -> String {
    let lower = s.trim().to_lowercase();
    let mut words: Vec<&str> = lower.split_whitespace().collect();
    if words.first() == Some(&"cb") {
        words.remove(0);
    }
    words.retain(|w| !is_date_like(w));
    words.join(" ")
}

/// Whether `word` looks like a `DD/MM/YY` or `DD/MM/YYYY` date (also
/// accepting `-` or `.` as the separator) rather than some other embedded
/// number, e.g. a reference number.
fn is_date_like(word: &str) -> bool {
    let parts: Vec<&str> = word.split(['/', '-', '.']).collect();
    let [day, month, year] = parts.as_slice() else {
        return false;
    };
    if day.is_empty() || day.len() > 2 || month.is_empty() || month.len() > 2 {
        return false;
    }
    if year.len() != 2 && year.len() != 4 {
        return false;
    }
    if !day.chars().all(|c| c.is_ascii_digit())
        || !month.chars().all(|c| c.is_ascii_digit())
        || !year.chars().all(|c| c.is_ascii_digit())
    {
        return false;
    }
    let (Ok(day), Ok(month)) = (day.parse::<u32>(), month.parse::<u32>()) else {
        return false;
    };
    (1..=31).contains(&day) && (1..=12).contains(&month)
}

/// Lowercases and splits on non-alphanumeric boundaries, dropping short
/// tokens (numbers, single letters) that are more noise than signal for a
/// description-text match.
fn tokenize(s: &str) -> std::collections::HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use scrat_domain::account::{Account, AccountName, DescriptionPattern};
    use scrat_domain::category::{Category, CategoryName};
    use scrat_domain::transfer_rule::TransferRuleId;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeAccountRepository {
        accounts: Mutex<Vec<Account>>,
        /// Standing in for the transactions table's per-account sum. Kept
        /// separate from `FakeTransactionRepository` on purpose: reconcile
        /// tests need to describe an account that already has a ledger
        /// history, without routing every prior movement through the
        /// service under test.
        ledger: Mutex<Vec<(AccountId, i64)>>,
    }

    impl FakeAccountRepository {
        fn post(&self, account_id: AccountId, minor_units: i64) {
            self.ledger.lock().unwrap().push((account_id, minor_units));
        }
    }

    impl AccountRepository for FakeAccountRepository {
        fn insert(&self, account: &Account) -> Result<(), RepositoryError> {
            self.accounts.lock().unwrap().push(account.clone());
            Ok(())
        }
        fn update(&self, _account: &Account) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn delete(&self, _id: AccountId) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError> {
            Ok(self
                .accounts
                .lock()
                .unwrap()
                .iter()
                .find(|a| a.id() == id)
                .cloned())
        }
        fn list_all(&self) -> Result<Vec<Account>, RepositoryError> {
            Ok(self.accounts.lock().unwrap().clone())
        }
        fn reorder(&self, _ordered_ids: &[AccountId]) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn transaction_count(&self, _id: AccountId) -> Result<u64, RepositoryError> {
            Ok(0)
        }
        fn sum_transactions_minor_units(&self, id: AccountId) -> Result<i64, RepositoryError> {
            Ok(self
                .ledger
                .lock()
                .unwrap()
                .iter()
                .filter(|(account_id, _)| *account_id == id)
                .map(|(_, minor_units)| minor_units)
                .sum())
        }
    }

    #[derive(Default)]
    struct FakeCategoryRepository {
        categories: Mutex<Vec<Category>>,
    }

    impl CategoryRepository for FakeCategoryRepository {
        fn insert(&self, category: &Category) -> Result<(), RepositoryError> {
            self.categories.lock().unwrap().push(category.clone());
            Ok(())
        }
        fn update(&self, _category: &Category) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn delete(&self, _id: CategoryId) -> Result<(), RepositoryError> {
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
            _from: CategoryId,
            _to: Option<CategoryId>,
        ) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn reassign_transactions(
            &self,
            _from: CategoryId,
            _to: CategoryId,
        ) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn transaction_count(&self, _id: CategoryId) -> Result<u64, RepositoryError> {
            Ok(0)
        }
    }

    #[derive(Default)]
    struct FakeTransactionRepository {
        transactions: Mutex<Vec<Transaction>>,
    }

    impl TransactionRepository for FakeTransactionRepository {
        fn insert(&self, transaction: &Transaction) -> Result<(), RepositoryError> {
            self.transactions.lock().unwrap().push(transaction.clone());
            Ok(())
        }
        fn delete(&self, id: TransactionId) -> Result<(), RepositoryError> {
            self.transactions.lock().unwrap().retain(|t| t.id() != id);
            Ok(())
        }
        fn find_by_id(&self, id: TransactionId) -> Result<Option<Transaction>, RepositoryError> {
            Ok(self
                .transactions
                .lock()
                .unwrap()
                .iter()
                .find(|t| t.id() == id)
                .cloned())
        }
        fn delete_transfer_group(&self, group_id: TransferGroupId) -> Result<(), RepositoryError> {
            self.transactions
                .lock()
                .unwrap()
                .retain(|t| t.transfer_group_id() != Some(group_id));
            Ok(())
        }
        fn delete_many(&self, ids: &[TransactionId]) -> Result<(), RepositoryError> {
            self.transactions
                .lock()
                .unwrap()
                .retain(|t| !ids.contains(&t.id()));
            Ok(())
        }
        fn update_category_many(
            &self,
            ids: &[TransactionId],
            category_id: CategoryId,
        ) -> Result<(), RepositoryError> {
            let mut transactions = self.transactions.lock().unwrap();
            for pos in 0..transactions.len() {
                if !ids.contains(&transactions[pos].id()) {
                    continue;
                }
                let existing = &transactions[pos];
                let updated = Transaction::new_with_role(
                    existing.id(),
                    existing.date(),
                    existing.amount().clone(),
                    existing.description().clone(),
                    category_id,
                    existing.account_id(),
                    existing.role(),
                    existing.transfer_group_id(),
                )
                .expect("recategorizing preserves validity");
                transactions[pos] = updated;
            }
            Ok(())
        }
        fn update_category(
            &self,
            id: TransactionId,
            category_id: CategoryId,
        ) -> Result<(), RepositoryError> {
            let mut transactions = self.transactions.lock().unwrap();
            if let Some(pos) = transactions.iter().position(|t| t.id() == id) {
                let existing = &transactions[pos];
                let updated = Transaction::new_with_role(
                    existing.id(),
                    existing.date(),
                    existing.amount().clone(),
                    existing.description().clone(),
                    category_id,
                    existing.account_id(),
                    existing.role(),
                    existing.transfer_group_id(),
                )
                .expect("recategorizing preserves validity");
                transactions[pos] = updated;
            }
            Ok(())
        }
        fn list_in_range(
            &self,
            start: NaiveDate,
            end: NaiveDate,
        ) -> Result<Vec<Transaction>, RepositoryError> {
            Ok(self
                .transactions
                .lock()
                .unwrap()
                .iter()
                .filter(|t| t.date() >= start && t.date() <= end)
                .cloned()
                .collect())
        }
        fn list_all(&self) -> Result<Vec<Transaction>, RepositoryError> {
            Ok(self.transactions.lock().unwrap().clone())
        }
        fn list_page(
            &self,
            offset: i64,
            limit: i64,
            filters: &TransactionFilters,
            sort_field: TransactionSortField,
            sort_dir: SortDirection,
        ) -> Result<Vec<Transaction>, RepositoryError> {
            let mut sorted: Vec<Transaction> = self
                .transactions
                .lock()
                .unwrap()
                .iter()
                .filter(|t| matches_filters(t, filters))
                .cloned()
                .collect();
            // This fake only ever sees a `Transaction`, which carries
            // `category_id`/`account_id` but not the linked aggregate's
            // name — unlike the real SQLite adapter, it can't join to
            // resolve one. Falling back to the id is only exercised by
            // tests that check filtering/pagination plumbing, not exact
            // alphabetical order, which is covered against real names in
            // `infra-sqlite`'s tests instead.
            let key = |t: &Transaction| -> String {
                match sort_field {
                    TransactionSortField::Date => t.date().to_string(),
                    TransactionSortField::Amount => {
                        format!("{:020}", t.amount().minor_units() + i64::MAX / 2)
                    }
                    TransactionSortField::Description => t.description().as_str().to_lowercase(),
                    TransactionSortField::OperationKind => t.operation_kind().as_str().to_string(),
                    TransactionSortField::Category => t.category_id().as_string(),
                    TransactionSortField::Account => t.account_id().as_string(),
                }
            };
            sorted.sort_by(|a, b| {
                let cmp = key(a).cmp(&key(b));
                let cmp = match sort_dir {
                    SortDirection::Asc => cmp,
                    SortDirection::Desc => cmp.reverse(),
                };
                cmp.then(match sort_dir {
                    SortDirection::Asc => a.id().as_string().cmp(&b.id().as_string()),
                    SortDirection::Desc => b.id().as_string().cmp(&a.id().as_string()),
                })
            });
            Ok(sorted
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect())
        }
        fn count_in_range(
            &self,
            start: NaiveDate,
            end: NaiveDate,
            filters: &TransactionFilters,
        ) -> Result<i64, RepositoryError> {
            Ok(self
                .transactions
                .lock()
                .unwrap()
                .iter()
                .filter(|t| t.date() >= start && t.date() <= end)
                .filter(|t| matches_filters(t, filters))
                .count() as i64)
        }
    }

    /// The predicate `list_page` and `count_in_range` both apply — kept as
    /// one function so the fake can't let the two drift apart the way
    /// separate inline filter chains once did.
    fn matches_filters(t: &Transaction, filters: &TransactionFilters) -> bool {
        filters.category_id.is_none_or(|id| t.category_id() == id)
            && filters.description_contains.as_ref().is_none_or(|s| {
                t.description()
                    .as_str()
                    .to_lowercase()
                    .contains(&s.to_lowercase())
            })
            && filters
                .is_income
                .is_none_or(|income| (t.amount().minor_units() > 0) == income)
            && filters.account_id.is_none_or(|id| t.account_id() == id)
            && filters
                .operation_kind
                .is_none_or(|kind| t.operation_kind() == kind)
            && filters
                .min_amount_minor_units
                .is_none_or(|min| t.amount().minor_units().unsigned_abs() as i64 >= min)
            && filters
                .max_amount_minor_units
                .is_none_or(|max| t.amount().minor_units().unsigned_abs() as i64 <= max)
    }

    /// Builds a `TransactionFilters` for the three filters most tests below
    /// exercise, leaving the newer account/type/amount fields at their
    /// "no filter" default.
    fn filters(
        category_id: Option<CategoryId>,
        description_contains: Option<&str>,
        is_income: Option<bool>,
    ) -> TransactionFilters {
        TransactionFilters {
            category_id,
            description_contains: description_contains.map(str::to_string),
            is_income,
            ..Default::default()
        }
    }

    struct Fixture {
        transactions: FakeTransactionRepository,
        accounts: FakeAccountRepository,
        categories: FakeCategoryRepository,
        account_id: AccountId,
        /// A second account standing in for one whose statements can't be
        /// exported: it receives mirrored transfer legs and gets reconciled.
        counterpart_account_id: AccountId,
        category_id: CategoryId,
    }

    fn fixture() -> Fixture {
        let accounts = FakeAccountRepository::default();
        let account = Account::new(
            AccountId::new(),
            AccountName::new("Checking").unwrap(),
            Money::zero(Currency::new("USD").unwrap()),
        );
        let account_id = account.id();
        accounts.insert(&account).unwrap();

        let counterpart = Account::new(
            AccountId::new(),
            AccountName::new("Neobank").unwrap(),
            Money::zero(Currency::new("USD").unwrap()),
        );
        let counterpart_account_id = counterpart.id();
        accounts.insert(&counterpart).unwrap();

        let categories = FakeCategoryRepository::default();
        let category = Category::new(
            CategoryId::new(),
            CategoryName::new("Groceries").unwrap(),
            None,
        )
        .unwrap();
        let category_id = category.id();
        categories.insert(&category).unwrap();

        Fixture {
            transactions: FakeTransactionRepository::default(),
            accounts,
            categories,
            account_id,
            counterpart_account_id,
            category_id,
        }
    }

    impl Fixture {
        fn service(&self) -> TransactionService<'_> {
            TransactionService::new(
                &self.transactions,
                &self.accounts,
                &self.categories,
                Currency::new("USD").unwrap(),
            )
        }

        fn import_row(&self, description: &str, amount_minor_units: i64) -> ImportRow {
            ImportRow {
                date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                amount_minor_units,
                description: description.to_string(),
                category_id: self.category_id,
                operation_kind: OperationKind::default(),
            }
        }

        fn transfer_rule(&self, pattern: &str, counterpart: AccountId) -> TransferRule {
            TransferRule::new(
                TransferRuleId::new(),
                DescriptionPattern::new(pattern).unwrap(),
                counterpart,
            )
        }
    }

    #[test]
    fn create_transaction_with_known_account_and_category_succeeds() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let transaction = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -1_200,
                "Whole Foods",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        assert_eq!(transaction.amount().minor_units(), -1_200);
    }

    #[test]
    fn create_transaction_rejects_unknown_category() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let result = service.create_transaction(
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            -1_200,
            "Whole Foods",
            CategoryId::new(),
            f.account_id,
        );

        assert!(matches!(result, Err(ApplicationError::CategoryNotFound)));
    }

    #[test]
    fn create_transaction_rejects_unknown_account() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let result = service.create_transaction(
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            -1_200,
            "Whole Foods",
            f.category_id,
            AccountId::new(),
        );

        assert!(matches!(result, Err(ApplicationError::AccountNotFound)));
    }

    #[test]
    fn set_category_updates_transaction_to_new_known_category() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        let transaction = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -1_200,
                "Whole Foods",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let dining = Category::new(
            CategoryId::new(),
            CategoryName::new("Dining").unwrap(),
            None,
        )
        .unwrap();
        f.categories.insert(&dining).unwrap();

        service.set_category(transaction.id(), dining.id()).unwrap();

        let stored = service.list_all().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].category_id(), dining.id());
    }

    #[test]
    fn set_category_rejects_unknown_category() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        let transaction = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -1_200,
                "Whole Foods",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let result = service.set_category(transaction.id(), CategoryId::new());

        assert!(matches!(result, Err(ApplicationError::CategoryNotFound)));
    }

    #[test]
    fn list_in_range_excludes_transactions_outside_range() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -1_200,
                "In range",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
                -500,
                "Out of range",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let results = service
            .list_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            )
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].description().as_str(), "In range");
    }

    #[test]
    fn count_in_range_reflects_the_description_filter() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -1_200,
                "Whole Foods Market",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
                -500,
                "Electric Co",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let count = service
            .count_in_range(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
                &filters(None, Some("whole foods"), None),
            )
            .unwrap();

        assert_eq!(count, 1);
    }

    /// The "All Time" view pages the ledger, so a category filter it can't
    /// push down here would only ever match inside the pages already
    /// fetched — the income rows below sit past the first page precisely to
    /// pin that down.
    #[test]
    fn list_page_narrows_to_the_requested_category() {
        let f = fixture();
        let salary = Category::new(
            CategoryId::new(),
            CategoryName::new("Salary").unwrap(),
            None,
        )
        .unwrap();
        f.categories.insert(&salary).unwrap();
        let service = f.service();
        for day in 1..=5 {
            service
                .create_transaction(
                    NaiveDate::from_ymd_opt(2026, 2, day).unwrap(),
                    -1_200,
                    &format!("Supermarket {day}"),
                    f.category_id,
                    f.account_id,
                )
                .unwrap();
        }
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
                250_000,
                "Employer",
                salary.id(),
                f.account_id,
            )
            .unwrap();

        let unfiltered = service
            .list_page(
                0,
                3,
                &filters(None, None, None),
                TransactionSortField::Date,
                SortDirection::Desc,
            )
            .unwrap();
        let filtered = service
            .list_page(
                0,
                3,
                &filters(Some(salary.id()), None, None),
                TransactionSortField::Date,
                SortDirection::Desc,
            )
            .unwrap();

        assert!(
            !unfiltered.iter().any(|t| t.category_id() == salary.id()),
            "the income row is past the first unfiltered page — otherwise \
             this test proves nothing"
        );
        assert_eq!(
            filtered
                .iter()
                .map(|t| t.description().as_str())
                .collect::<Vec<_>>(),
            vec!["Employer"]
        );
    }

    #[test]
    fn list_page_narrows_to_the_requested_description() {
        let f = fixture();
        let service = f.service();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -1_200,
                "Whole Foods Market",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
                -500,
                "Electric Co",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let page = service
            .list_page(
                0,
                10,
                &filters(None, Some("whole foods"), None),
                TransactionSortField::Date,
                SortDirection::Desc,
            )
            .unwrap();

        assert_eq!(page.len(), 1);
        assert_eq!(page[0].description().as_str(), "Whole Foods Market");
    }

    /// The Expenses and Income lists page the ledger independently — each
    /// asks the service for just its own sign, rather than fetching a mixed
    /// batch and splitting it after the fact.
    #[test]
    fn list_page_narrows_to_the_requested_sign() {
        let f = fixture();
        let service = f.service();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -1_200,
                "Whole Foods Market",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
                250_000,
                "Employer",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let income = service
            .list_page(
                0,
                10,
                &filters(None, None, Some(true)),
                TransactionSortField::Date,
                SortDirection::Desc,
            )
            .unwrap();
        let expenses = service
            .list_page(
                0,
                10,
                &filters(None, None, Some(false)),
                TransactionSortField::Date,
                SortDirection::Desc,
            )
            .unwrap();

        assert_eq!(
            income
                .iter()
                .map(|t| t.description().as_str())
                .collect::<Vec<_>>(),
            vec!["Employer"]
        );
        assert_eq!(
            expenses
                .iter()
                .map(|t| t.description().as_str())
                .collect::<Vec<_>>(),
            vec!["Whole Foods Market"]
        );
    }

    #[test]
    fn find_account_by_description_matches_saved_pattern() {
        let f = fixture();
        let mut account = f.accounts.accounts.lock().unwrap()[0].clone();
        account.add_description_pattern(
            scrat_domain::account::DescriptionPattern::new("whole foods").unwrap(),
        );
        f.accounts.accounts.lock().unwrap()[0] = account;
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let found = service
            .find_account_by_description("WHOLE FOODS #42")
            .unwrap();

        assert_eq!(found, Some(f.account_id));
    }

    /// The instrument the importer read has to reach storage. It travels
    /// through a builder call rather than the constructor, which makes it
    /// exactly the kind of field a refactor drops without anything failing
    /// to compile.
    #[test]
    fn import_transactions_stores_each_rows_operation_kind() {
        let f = fixture();
        let service = f.service();
        let mut fees = f.import_row("FRAIS TENUE DE COMPTE", -200);
        fees.operation_kind = OperationKind::Fees;
        let card = f.import_row("SOME STORE", -1_200);

        service
            .import_transactions(&[fees, card], f.account_id, &[])
            .unwrap();

        let stored = service.list_all().unwrap();
        let by_description = |text: &str| {
            stored
                .iter()
                .find(|t| t.description().as_str() == text)
                .unwrap()
                .operation_kind()
        };
        assert_eq!(by_description("FRAIS TENUE DE COMPTE"), OperationKind::Fees);
        assert_eq!(by_description("SOME STORE"), OperationKind::Card);
    }

    /// A row that a transfer rule turns into a pair must put the instrument
    /// on *both* legs — the counterpart is the same movement seen from the
    /// other account, and it's written onto an account the user isn't even
    /// looking at, so a gap there would be invisible.
    #[test]
    fn an_imported_transfer_pair_carries_the_operation_kind_onto_both_legs() {
        let f = fixture();
        let service = f.service();
        let mut row = f.import_row("VIREMENT NEOBANK", -25_000);
        row.operation_kind = OperationKind::BankTransfer;
        let rule = f.transfer_rule("virement neobank", f.counterpart_account_id);

        let outcome = service
            .import_transactions(&[row], f.account_id, &[rule])
            .unwrap();

        assert_eq!(outcome.mirrored, 1);
        let stored = service.list_all().unwrap();
        assert_eq!(stored.len(), 2);
        assert!(
            stored
                .iter()
                .all(|t| t.operation_kind() == OperationKind::BankTransfer)
        );
    }

    #[test]
    fn import_transactions_keeps_identical_rows_as_separate_transactions() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        let row = ImportRow {
            date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            amount_minor_units: -1_200,
            description: "Whole Foods".to_string(),
            category_id: f.category_id,
            operation_kind: OperationKind::default(),
        };

        let first = service
            .import_transactions(&[row.clone(), row.clone()], f.account_id, &[])
            .unwrap();
        assert_eq!(
            first,
            ImportOutcome {
                imported: 2,
                mirrored: 0
            }
        );

        // Re-importing the same "file" again adds two more — this port does
        // no deduplication, that's the caller's responsibility.
        let second = service
            .import_transactions(&[row], f.account_id, &[])
            .unwrap();
        assert_eq!(
            second,
            ImportOutcome {
                imported: 1,
                mirrored: 0
            }
        );
    }

    #[test]
    fn import_writes_a_mirrored_leg_on_the_counterpart_account() {
        let f = fixture();
        let service = f.service();
        let rules = [f.transfer_rule("n26", f.counterpart_account_id)];

        let outcome = service
            .import_transactions(
                &[f.import_row("VIREMENT SEPA EMIS VERS N26", -25_000)],
                f.account_id,
                &rules,
            )
            .unwrap();

        assert_eq!(
            outcome,
            ImportOutcome {
                imported: 1,
                mirrored: 1
            }
        );

        let all = f.transactions.list_all().unwrap();
        assert_eq!(all.len(), 2);
        let outflow = all
            .iter()
            .find(|t| t.account_id() == f.account_id)
            .expect("origin account leg");
        let inflow = all
            .iter()
            .find(|t| t.account_id() == f.counterpart_account_id)
            .expect("counterpart leg");

        assert_eq!(outflow.amount().minor_units(), -25_000);
        assert_eq!(inflow.amount().minor_units(), 25_000);
        assert_eq!(outflow.role(), TransactionRole::Transfer);
        assert_eq!(inflow.role(), TransactionRole::Transfer);
        assert_eq!(outflow.transfer_group_id(), inflow.transfer_group_id());
        assert!(outflow.transfer_group_id().is_some());
    }

    /// The user's own money coming back the other way is still a transfer,
    /// so the same rule has to work on an inflow without special-casing.
    #[test]
    fn import_mirrors_an_incoming_transfer_in_the_opposite_direction() {
        let f = fixture();
        let service = f.service();
        let rules = [f.transfer_rule("virement n26", f.counterpart_account_id)];

        service
            .import_transactions(
                &[f.import_row("VIREMENT N26 VERS COMPTE", 12_000)],
                f.account_id,
                &rules,
            )
            .unwrap();

        let all = f.transactions.list_all().unwrap();
        let inflow = all
            .iter()
            .find(|t| t.account_id() == f.account_id)
            .expect("origin account leg");
        let outflow = all
            .iter()
            .find(|t| t.account_id() == f.counterpart_account_id)
            .expect("counterpart leg");
        assert_eq!(inflow.amount().minor_units(), 12_000);
        assert_eq!(outflow.amount().minor_units(), -12_000);
    }

    #[test]
    fn import_leaves_rows_no_rule_matches_as_ordinary_transactions() {
        let f = fixture();
        let service = f.service();
        let rules = [f.transfer_rule("virement n26", f.counterpart_account_id)];

        let outcome = service
            .import_transactions(
                &[f.import_row("CARTE 12/03 BOULANGERIE", -450)],
                f.account_id,
                &rules,
            )
            .unwrap();

        assert_eq!(
            outcome,
            ImportOutcome {
                imported: 1,
                mirrored: 0
            }
        );
        let all = f.transactions.list_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].role(), TransactionRole::Normal);
        assert_eq!(all[0].transfer_group_id(), None);
    }

    /// A rule pointing back at the account being imported would mirror the
    /// row onto itself: two entries on one account that cancel out, leaving
    /// its balance unchanged by a movement that really happened.
    #[test]
    fn import_ignores_a_rule_whose_counterpart_is_the_imported_account() {
        let f = fixture();
        let service = f.service();
        let rules = [f.transfer_rule("virement n26", f.account_id)];

        let outcome = service
            .import_transactions(
                &[f.import_row("VIREMENT N26", -25_000)],
                f.account_id,
                &rules,
            )
            .unwrap();

        assert_eq!(
            outcome,
            ImportOutcome {
                imported: 1,
                mirrored: 0
            }
        );
        let all = f.transactions.list_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].role(), TransactionRole::Normal);
    }

    #[test]
    fn import_rejects_a_rule_pointing_at_an_account_that_no_longer_exists() {
        let f = fixture();
        let service = f.service();
        let rules = [f.transfer_rule("virement n26", AccountId::new())];

        let result = service.import_transactions(
            &[f.import_row("VIREMENT N26", -25_000)],
            f.account_id,
            &rules,
        );

        assert!(matches!(result, Err(ApplicationError::AccountNotFound)));
    }

    #[test]
    fn deleting_either_leg_of_a_transfer_removes_both() {
        let f = fixture();
        let service = f.service();
        let rules = [f.transfer_rule("virement n26", f.counterpart_account_id)];
        service
            .import_transactions(
                &[f.import_row("VIREMENT N26", -25_000)],
                f.account_id,
                &rules,
            )
            .unwrap();
        let inflow_id = f
            .transactions
            .list_all()
            .unwrap()
            .iter()
            .find(|t| t.account_id() == f.counterpart_account_id)
            .expect("counterpart leg")
            .id();

        // Delete the mirrored leg, not the one the import created first.
        service.delete_transaction(inflow_id).unwrap();

        assert!(f.transactions.list_all().unwrap().is_empty());
    }

    #[test]
    fn deleting_an_ordinary_transaction_leaves_everything_else_alone() {
        let f = fixture();
        let service = f.service();
        let kept = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -450,
                "Boulangerie",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let removed = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
                -900,
                "Pharmacie",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        service.delete_transaction(removed.id()).unwrap();

        let all = f.transactions.list_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id(), kept.id());
    }

    #[test]
    fn delete_transactions_removes_every_listed_ordinary_transaction() {
        let f = fixture();
        let service = f.service();
        let a = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -450,
                "Boulangerie",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let b = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
                -900,
                "Pharmacie",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let kept = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 17).unwrap(),
                -300,
                "Cafe",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let outcome = service.delete_transactions(&[a.id(), b.id()]).unwrap();

        assert_eq!(
            outcome,
            BulkDeleteOutcome {
                deleted: 2,
                transfer_groups: 0
            }
        );
        let all = f.transactions.list_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id(), kept.id());
    }

    #[test]
    fn delete_transactions_expands_a_single_selected_leg_to_the_whole_transfer_group() {
        let f = fixture();
        let service = f.service();
        let rules = [f.transfer_rule("virement n26", f.counterpart_account_id)];
        service
            .import_transactions(
                &[f.import_row("VIREMENT N26", -25_000)],
                f.account_id,
                &rules,
            )
            .unwrap();
        let outflow_id = f
            .transactions
            .list_all()
            .unwrap()
            .iter()
            .find(|t| t.account_id() == f.account_id)
            .expect("origin leg")
            .id();

        // Only the outflow leg is in the selection — the inflow leg was
        // never loaded on the counterpart account's own page.
        let outcome = service.delete_transactions(&[outflow_id]).unwrap();

        assert_eq!(
            outcome,
            BulkDeleteOutcome {
                deleted: 2,
                transfer_groups: 1
            }
        );
        assert!(f.transactions.list_all().unwrap().is_empty());
    }

    #[test]
    fn delete_transactions_with_both_legs_selected_deletes_the_pair_once() {
        let f = fixture();
        let service = f.service();
        let rules = [f.transfer_rule("virement n26", f.counterpart_account_id)];
        service
            .import_transactions(
                &[f.import_row("VIREMENT N26", -25_000)],
                f.account_id,
                &rules,
            )
            .unwrap();
        let legs = f.transactions.list_all().unwrap();
        let outflow_id = legs
            .iter()
            .find(|t| t.account_id() == f.account_id)
            .unwrap()
            .id();
        let inflow_id = legs
            .iter()
            .find(|t| t.account_id() == f.counterpart_account_id)
            .unwrap()
            .id();

        let outcome = service
            .delete_transactions(&[outflow_id, inflow_id])
            .unwrap();

        assert_eq!(
            outcome,
            BulkDeleteOutcome {
                deleted: 2,
                transfer_groups: 1
            }
        );
        assert!(f.transactions.list_all().unwrap().is_empty());
    }

    #[test]
    fn delete_transactions_with_empty_list_is_a_no_op() {
        let f = fixture();
        let service = f.service();
        let kept = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -450,
                "Boulangerie",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let outcome = service.delete_transactions(&[]).unwrap();

        assert_eq!(
            outcome,
            BulkDeleteOutcome {
                deleted: 0,
                transfer_groups: 0
            }
        );
        let all = f.transactions.list_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id(), kept.id());
    }

    #[test]
    fn set_category_many_updates_every_listed_transaction() {
        let f = fixture();
        let service = f.service();
        let a = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -450,
                "Boulangerie",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let b = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
                -900,
                "Pharmacie",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let untouched = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 17).unwrap(),
                -300,
                "Cafe",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let dining = Category::new(
            CategoryId::new(),
            CategoryName::new("Dining").unwrap(),
            None,
        )
        .unwrap();
        f.categories.insert(&dining).unwrap();

        service
            .set_category_many(&[a.id(), b.id()], dining.id())
            .unwrap();

        let all = f.transactions.list_all().unwrap();
        assert_eq!(
            all.iter().find(|t| t.id() == a.id()).unwrap().category_id(),
            dining.id()
        );
        assert_eq!(
            all.iter().find(|t| t.id() == b.id()).unwrap().category_id(),
            dining.id()
        );
        assert_eq!(
            all.iter()
                .find(|t| t.id() == untouched.id())
                .unwrap()
                .category_id(),
            f.category_id
        );
    }

    #[test]
    fn set_category_many_rejects_unknown_category_and_writes_nothing() {
        let f = fixture();
        let service = f.service();
        let a = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -450,
                "Boulangerie",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let result = service.set_category_many(&[a.id()], CategoryId::new());

        assert!(matches!(result, Err(ApplicationError::CategoryNotFound)));
        let all = f.transactions.list_all().unwrap();
        assert_eq!(all[0].category_id(), f.category_id);
    }

    #[test]
    fn retroactive_apply_converts_a_matching_normal_transaction_into_a_mirrored_pair() {
        let f = fixture();
        let service = f.service();
        let existing = service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -25_000,
                "VIREMENT N26",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let rules = [f.transfer_rule("n26", f.counterpart_account_id)];

        let outcome = service.apply_transfer_rules_to_existing(&rules).unwrap();

        assert_eq!(outcome, RetroactiveTransferOutcome { converted: 1 });
        let all = f.transactions.list_all().unwrap();
        assert_eq!(all.len(), 2);
        let outflow = all
            .iter()
            .find(|t| t.account_id() == f.account_id)
            .expect("origin account leg");
        let inflow = all
            .iter()
            .find(|t| t.account_id() == f.counterpart_account_id)
            .expect("counterpart leg");
        // The row keeps its original id — it's reclassified, not replaced.
        assert_eq!(outflow.id(), existing.id());
        assert_eq!(outflow.amount().minor_units(), -25_000);
        assert_eq!(inflow.amount().minor_units(), 25_000);
        assert_eq!(outflow.role(), TransactionRole::Transfer);
        assert_eq!(outflow.transfer_group_id(), inflow.transfer_group_id());
    }

    #[test]
    fn retroactive_apply_leaves_non_matching_transactions_untouched() {
        let f = fixture();
        let service = f.service();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -450,
                "Boulangerie",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let rules = [f.transfer_rule("n26", f.counterpart_account_id)];

        let outcome = service.apply_transfer_rules_to_existing(&rules).unwrap();

        assert_eq!(outcome, RetroactiveTransferOutcome { converted: 0 });
        let all = f.transactions.list_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].role(), TransactionRole::Normal);
    }

    /// Running this a second time (e.g. after adding another rule) must not
    /// touch a row it already converted — the pair it wrote is now a
    /// `Transfer`, which never enters the matching branch again.
    #[test]
    fn retroactive_apply_does_not_reconvert_a_row_it_already_converted() {
        let f = fixture();
        let service = f.service();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -25_000,
                "VIREMENT N26",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let rules = [f.transfer_rule("n26", f.counterpart_account_id)];
        service.apply_transfer_rules_to_existing(&rules).unwrap();

        let second_pass = service.apply_transfer_rules_to_existing(&rules).unwrap();

        assert_eq!(second_pass, RetroactiveTransferOutcome { converted: 0 });
        assert_eq!(f.transactions.list_all().unwrap().len(), 2);
    }

    /// A rule pointing back at the row's own account would mirror it onto
    /// itself — the same guard `import_transactions` applies.
    #[test]
    fn retroactive_apply_ignores_a_rule_whose_counterpart_is_the_rows_own_account() {
        let f = fixture();
        let service = f.service();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                -25_000,
                "VIREMENT N26",
                f.category_id,
                f.account_id,
            )
            .unwrap();
        let rules = [f.transfer_rule("n26", f.account_id)];

        let outcome = service.apply_transfer_rules_to_existing(&rules).unwrap();

        assert_eq!(outcome, RetroactiveTransferOutcome { converted: 0 });
        let all = f.transactions.list_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].role(), TransactionRole::Normal);
    }

    #[test]
    fn reconcile_posts_the_difference_between_observed_and_ledger_balance() {
        let f = fixture();
        // The counterpart has received 250.00 in transfers, but the user
        // spent 40.00 from it in ways no export can show.
        f.accounts.post(f.counterpart_account_id, 25_000);
        let service = f.service();

        let adjustment = service
            .reconcile_account(
                f.counterpart_account_id,
                21_000,
                f.category_id,
                NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            )
            .unwrap()
            .expect("a drifted balance produces an adjustment");

        assert_eq!(adjustment.amount().minor_units(), -4_000);
        assert_eq!(adjustment.role(), TransactionRole::Adjustment);
        assert_eq!(adjustment.account_id(), f.counterpart_account_id);
        assert_eq!(
            adjustment.description().as_str(),
            RECONCILIATION_DESCRIPTION
        );
        assert_eq!(f.transactions.list_all().unwrap().len(), 1);
    }

    #[test]
    fn reconcile_counts_the_opening_balance_not_just_the_ledger() {
        let f = fixture();
        let opening = Account::new(
            AccountId::new(),
            AccountName::new("Neobank with history").unwrap(),
            Money::from_minor_units(10_000, Currency::new("USD").unwrap()),
        );
        let account_id = opening.id();
        f.accounts.insert(&opening).unwrap();
        f.accounts.post(account_id, 5_000);
        let service = f.service();

        let adjustment = service
            .reconcile_account(
                account_id,
                20_000,
                f.category_id,
                NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            )
            .unwrap()
            .expect("a drifted balance produces an adjustment");

        // 200.00 observed against 100.00 opening + 50.00 ledger.
        assert_eq!(adjustment.amount().minor_units(), 5_000);
    }

    /// Checking an account that needs nothing shouldn't leave a trail of
    /// zero-value entries — and a zero-amount transaction isn't even a
    /// valid one.
    #[test]
    fn reconcile_writes_nothing_when_the_balance_already_agrees() {
        let f = fixture();
        f.accounts.post(f.counterpart_account_id, 25_000);
        let service = f.service();

        let adjustment = service
            .reconcile_account(
                f.counterpart_account_id,
                25_000,
                f.category_id,
                NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            )
            .unwrap();

        assert!(adjustment.is_none());
        assert!(f.transactions.list_all().unwrap().is_empty());
    }

    /// The observed balance is typed in by hand, so a slip of the keyboard
    /// must not wrap a balance around into a plausible-looking number.
    #[test]
    fn reconcile_rejects_an_observed_balance_that_would_overflow() {
        let f = fixture();
        f.accounts.post(f.counterpart_account_id, -1_000);
        let service = f.service();

        let result = service.reconcile_account(
            f.counterpart_account_id,
            i64::MAX,
            f.category_id,
            NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
        );

        assert!(matches!(result, Err(ApplicationError::BalanceOutOfRange)));
    }

    #[test]
    fn reconcile_rejects_an_unknown_account() {
        let f = fixture();
        let service = f.service();

        let result = service.reconcile_account(
            AccountId::new(),
            21_000,
            f.category_id,
            NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
        );

        assert!(matches!(result, Err(ApplicationError::AccountNotFound)));
    }

    #[test]
    fn import_transactions_rejects_unknown_account() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        let row = ImportRow {
            date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            amount_minor_units: -1_200,
            description: "Whole Foods".to_string(),
            category_id: f.category_id,
            operation_kind: OperationKind::default(),
        };

        let result = service.import_transactions(&[row], AccountId::new(), &[]);

        assert!(matches!(result, Err(ApplicationError::AccountNotFound)));
    }

    #[test]
    fn import_transactions_rejects_unknown_category() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        let row = ImportRow {
            date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            amount_minor_units: -1_200,
            description: "Whole Foods".to_string(),
            category_id: CategoryId::new(),
            operation_kind: OperationKind::default(),
        };

        let result = service.import_transactions(&[row], f.account_id, &[]);

        assert!(matches!(result, Err(ApplicationError::CategoryNotFound)));
    }

    #[test]
    fn get_or_create_category_by_name_creates_new_top_level_category() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let id = service.get_or_create_category_by_name("Books").unwrap();

        let stored = f.categories.find_by_id(id).unwrap().unwrap();
        assert_eq!(stored.name().as_str(), "Books");
        assert_eq!(stored.parent_id(), None);
    }

    #[test]
    fn get_or_create_category_by_name_matches_existing_case_insensitively() {
        let f = fixture(); // fixture already has a "Groceries" category
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let id = service.get_or_create_category_by_name("groceries").unwrap();

        assert_eq!(id, f.category_id);
        assert_eq!(f.categories.categories.lock().unwrap().len(), 1); // no duplicate created
    }

    #[test]
    fn get_or_create_category_path_creates_category_and_subcategory() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let id = service
            .get_or_create_category_path("Education", Some("Books"))
            .unwrap();

        let stored = f.categories.find_by_id(id).unwrap().unwrap();
        assert_eq!(stored.name().as_str(), "Books");
        let parent = f
            .categories
            .find_by_id(stored.parent_id().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(parent.name().as_str(), "Education");
        assert_eq!(parent.parent_id(), None);
    }

    #[test]
    fn get_or_create_category_path_reuses_existing_category_and_subcategory() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        let first = service
            .get_or_create_category_path("Education", Some("Books"))
            .unwrap();

        let second = service
            .get_or_create_category_path("education", Some("books"))
            .unwrap();

        assert_eq!(first, second);
        // fixture's pre-seeded "Groceries" + new Education + new Books, no duplicates
        assert_eq!(f.categories.categories.lock().unwrap().len(), 3);
    }

    #[test]
    fn get_or_create_category_path_without_subcategory_resolves_top_level_category() {
        let f = fixture(); // fixture already has a "Groceries" category
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let id = service
            .get_or_create_category_path("Groceries", None)
            .unwrap();

        assert_eq!(id, f.category_id);
        assert_eq!(f.categories.categories.lock().unwrap().len(), 1); // no duplicate created
    }

    #[test]
    fn suggest_category_for_description_returns_most_common_category_among_matches() {
        let f = fixture();
        let entertainment = Category::new(
            CategoryId::new(),
            CategoryName::new("Entertainment").unwrap(),
            None,
        )
        .unwrap();
        f.categories.insert(&entertainment).unwrap();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        // Two past Netflix charges were (correctly) filed as Entertainment,
        // one was miscategorized as Groceries — Entertainment should win.
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                -1_500,
                "NETFLIX.COM",
                entertainment.id(),
                f.account_id,
            )
            .unwrap();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
                -1_500,
                "Netflix Subscription",
                entertainment.id(),
                f.account_id,
            )
            .unwrap();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
                -1_500,
                "netflix oops",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let suggestion = service
            .suggest_category_for_description("NETFLIX.COM")
            .unwrap();

        assert_eq!(suggestion, Some(entertainment.id()));
    }

    #[test]
    fn find_category_for_description_uses_most_recent_transactions_category() {
        let f = fixture();
        let dining = Category::new(
            CategoryId::new(),
            CategoryName::new("Dining").unwrap(),
            None,
        )
        .unwrap();
        f.categories.insert(&dining).unwrap();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        // Same description, filed under Groceries first, then recategorized (in a
        // later transaction) as Dining — the later one should win even
        // though it's not the last one inserted below.
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
                -1_000,
                "Corner Bistro",
                dining.id(),
                f.account_id,
            )
            .unwrap();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                -1_000,
                "Corner Bistro",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let found = service
            .find_category_for_description("corner bistro")
            .unwrap();

        assert_eq!(found, Some(dining.id()));
    }

    #[test]
    fn find_category_for_description_requires_exact_normalized_match() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                -1_000,
                "Corner Bistro Downtown",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let found = service
            .find_category_for_description("Corner Bistro")
            .unwrap();

        assert_eq!(found, None);
    }

    #[test]
    fn find_category_for_description_ignores_a_leading_cb_prefix() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                -1_000,
                "Corner Bistro",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        // The bank started prefixing card-payment descriptions with "CB" —
        // history recorded before that change should still match.
        let found = service
            .find_category_for_description("CB Corner Bistro")
            .unwrap();

        assert_eq!(found, Some(f.category_id));
    }

    #[test]
    fn find_category_for_description_ignores_an_embedded_date() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                -1_000,
                "CB SOME STORE 30/06/26",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        // Same merchant, different day the bank stamped into the text — the
        // date shouldn't stop history from matching.
        let found = service
            .find_category_for_description("CB SOME STORE 02/07/26")
            .unwrap();

        assert_eq!(found, Some(f.category_id));
    }

    #[test]
    fn find_category_for_description_returns_none_when_no_past_matches() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let found = service
            .find_category_for_description("Corner Bistro")
            .unwrap();

        assert_eq!(found, None);
    }

    #[test]
    fn list_for_account_returns_only_that_accounts_transactions() {
        let f = fixture();
        let service = f.service();
        let date = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        service
            .create_transaction(date, -1_500, "Whole Foods", f.category_id, f.account_id)
            .unwrap();
        service
            .create_transaction(
                date,
                -900,
                "Corner Bistro",
                f.category_id,
                f.counterpart_account_id,
            )
            .unwrap();

        let listed = service.list_for_account(f.account_id).unwrap();

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].description().as_str(), "Whole Foods");
    }

    /// The mirrored leg belongs to the counterpart account, so exporting that
    /// account exports it. Scoping an export by account narrows *which*
    /// transfer legs come out; it doesn't leave them out. Same for an
    /// `Adjustment`, covered below — both are real rows on the account, and
    /// dropping them would make the file disagree with the app's balance.
    #[test]
    fn list_for_account_includes_a_mirrored_transfer_leg_on_the_counterpart() {
        let f = fixture();
        let service = f.service();
        let rules = vec![f.transfer_rule("neobank", f.counterpart_account_id)];

        service
            .import_transactions(
                &[f.import_row("VIREMENT NEOBANK", -20_000)],
                f.account_id,
                &rules,
            )
            .unwrap();

        let counterpart = service.list_for_account(f.counterpart_account_id).unwrap();

        assert_eq!(counterpart.len(), 1);
        assert_eq!(counterpart[0].role(), TransactionRole::Transfer);
        assert_eq!(counterpart[0].amount().minor_units(), 20_000);
    }

    #[test]
    fn list_for_account_includes_an_adjustment_posted_by_reconciling() {
        let f = fixture();
        let service = f.service();
        let date = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        service
            .reconcile_account(f.counterpart_account_id, 12_345, f.category_id, date)
            .unwrap()
            .expect("a non-zero delta must post an adjustment");

        let listed = service.list_for_account(f.counterpart_account_id).unwrap();

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].role(), TransactionRole::Adjustment);
    }

    #[test]
    fn list_for_account_rejects_an_unknown_account() {
        let f = fixture();

        let result = f.service().list_for_account(AccountId::new());

        assert!(matches!(result, Err(ApplicationError::AccountNotFound)));
    }

    #[test]
    fn find_duplicate_rows_flags_a_row_matching_an_existing_transaction() {
        let f = fixture();
        let service = f.service();
        let date = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        service
            .create_transaction(date, -1_500, "Whole Foods", f.category_id, f.account_id)
            .unwrap();

        let flags = service
            .find_duplicate_rows(f.account_id, &[(date, -1_500, "Whole Foods".to_string())])
            .unwrap();

        assert_eq!(flags, vec![true]);
    }

    #[test]
    fn find_duplicate_rows_ignores_a_match_on_a_different_account() {
        let f = fixture();
        let service = f.service();
        let date = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        service
            .create_transaction(date, -1_500, "Whole Foods", f.category_id, f.account_id)
            .unwrap();

        // Same date/amount/description, but checked against a different
        // account than the one the existing transaction lives on.
        let flags = service
            .find_duplicate_rows(
                f.counterpart_account_id,
                &[(date, -1_500, "Whole Foods".to_string())],
            )
            .unwrap();

        assert_eq!(flags, vec![false]);
    }

    #[test]
    fn find_duplicate_rows_does_not_flag_rows_that_differ() {
        let f = fixture();
        let service = f.service();
        let date = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        service
            .create_transaction(date, -1_500, "Whole Foods", f.category_id, f.account_id)
            .unwrap();

        let flags = service
            .find_duplicate_rows(
                f.account_id,
                &[
                    // Different amount.
                    (date, -1_600, "Whole Foods".to_string()),
                    // Different date.
                    (
                        NaiveDate::from_ymd_opt(2026, 3, 2).unwrap(),
                        -1_500,
                        "Whole Foods".to_string(),
                    ),
                    // Different description.
                    (date, -1_500, "Trader Joe's".to_string()),
                ],
            )
            .unwrap();

        assert_eq!(flags, vec![false, false, false]);
    }

    #[test]
    fn find_duplicate_rows_normalizes_description_case_and_whitespace() {
        let f = fixture();
        let service = f.service();
        let date = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        service
            .create_transaction(date, -1_500, "Whole   Foods", f.category_id, f.account_id)
            .unwrap();

        let flags = service
            .find_duplicate_rows(
                f.account_id,
                &[(date, -1_500, "  whole foods  ".to_string())],
            )
            .unwrap();

        assert_eq!(flags, vec![true]);
    }

    #[test]
    fn find_category_for_description_in_category_returns_most_recent_subcategory() {
        let f = fixture();
        let home =
            Category::new(CategoryId::new(), CategoryName::new("Home").unwrap(), None).unwrap();
        f.categories.insert(&home).unwrap();
        let rent = Category::new(
            CategoryId::new(),
            CategoryName::new("Rent").unwrap(),
            Some(home.id()),
        )
        .unwrap();
        f.categories.insert(&rent).unwrap();
        let utilities = Category::new(
            CategoryId::new(),
            CategoryName::new("Utilities").unwrap(),
            Some(home.id()),
        )
        .unwrap();
        f.categories.insert(&utilities).unwrap();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        // Same description, first filed under Utilities, later recategorized to
        // Rent — the later one should win.
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                -1_000,
                "ACME Landlord",
                utilities.id(),
                f.account_id,
            )
            .unwrap();
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
                -1_000,
                "ACME Landlord",
                rent.id(),
                f.account_id,
            )
            .unwrap();

        let found = service
            .find_category_for_description_in_category("ACME Landlord", "Home")
            .unwrap();

        assert_eq!(found, Some(rent.id()));
    }

    #[test]
    fn find_category_for_description_in_category_ignores_matches_under_a_different_category() {
        let f = fixture();
        let home =
            Category::new(CategoryId::new(), CategoryName::new("Home").unwrap(), None).unwrap();
        f.categories.insert(&home).unwrap();
        let rent = Category::new(
            CategoryId::new(),
            CategoryName::new("Rent").unwrap(),
            Some(home.id()),
        )
        .unwrap();
        f.categories.insert(&rent).unwrap();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                -1_000,
                "ACME Landlord",
                rent.id(),
                f.account_id,
            )
            .unwrap();

        // fixture's pre-seeded "Groceries" category is unrelated to Home/Rent
        let found = service
            .find_category_for_description_in_category("ACME Landlord", "Groceries")
            .unwrap();

        assert_eq!(found, None);
    }

    #[test]
    fn find_category_for_description_in_category_matches_the_bare_top_level_category_itself() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        service
            .create_transaction(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                -1_200,
                "Whole Foods",
                f.category_id,
                f.account_id,
            )
            .unwrap();

        let found = service
            .find_category_for_description_in_category("Whole Foods", "Groceries")
            .unwrap();

        assert_eq!(found, Some(f.category_id));
    }

    #[test]
    fn suggest_category_for_description_returns_none_when_no_past_matches() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );

        let suggestion = service
            .suggest_category_for_description("Totally Unseen Merchant")
            .unwrap();

        assert_eq!(suggestion, None);
    }

    #[test]
    fn detect_recurring_charges_finds_a_subscription_in_the_ledger() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        for month in 4..=6 {
            service
                .create_transaction(
                    NaiveDate::from_ymd_opt(2026, month, 12).unwrap(),
                    -1_349,
                    "NETFLIX.COM",
                    f.category_id,
                    f.account_id,
                )
                .unwrap();
        }

        let charges = service
            .detect_recurring_charges(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 6, 20).unwrap(),
            )
            .unwrap();

        assert_eq!(charges.len(), 1);
        assert_eq!(charges[0].typical_amount_minor_units, 1_349);
        assert_eq!(charges[0].category_id, f.category_id);
    }

    /// The lookback is what the use-case actually contributes over the domain
    /// detector, so it's what this layer has to prove: occurrences outside the
    /// window must not reach detection, even though they're in the ledger.
    #[test]
    fn detect_recurring_charges_only_sees_the_requested_window() {
        let f = fixture();
        let service = TransactionService::new(
            &f.transactions,
            &f.accounts,
            &f.categories,
            Currency::new("USD").unwrap(),
        );
        for month in 4..=6 {
            service
                .create_transaction(
                    NaiveDate::from_ymd_opt(2026, month, 12).unwrap(),
                    -1_349,
                    "NETFLIX.COM",
                    f.category_id,
                    f.account_id,
                )
                .unwrap();
        }

        // Starting in May leaves only two occurrences in range — one short of
        // what detection requires.
        let charges = service
            .detect_recurring_charges(
                NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 6, 20).unwrap(),
            )
            .unwrap();

        assert!(charges.is_empty());
    }
}
