# Spec — Transaction multi-selection and bulk actions

Status: draft, not implemented.
Scope: `frontend/src/routes/(app)/transactions/+page.svelte` plus a bulk path
through `src-tauri` → `application` → `domain::ports` → `infra-sqlite`.

## 1. Goal

Let the user act on many transactions at once on the Transactions page:
select rows with a checkbox, then delete them or move them all to one
category. Today both operations exist only one row at a time
(`DeleteButton` per row, `CategorySelect` per row), which makes correcting a
mis-imported CSV batch a few hundred clicks.

## 2. Interpretation of the request (read before implementing)

Two points in the feature description are ambiguous; this spec resolves them
as follows. Flag now if either reading is wrong — they change the work
substantially.

- **"pen icon — rename categories for selected transactions"** is read as
  *reassign* the selected transactions to one existing category, i.e. the
  bulk form of today's per-row `CategorySelect` (`set_transaction_category`).
  It is **not** read as renaming the category entity itself
  (`rename_category`), which would rename it for every transaction in the
  ledger, not just the selection, and already exists on the Categories page.
- **"every transaction currently loaded"** is read as *every row currently
  rendered in that one list* — i.e. after the active category/source filters,
  and in "All Time" only the pages fetched so far. Not "every transaction in
  the database matching the range". See §4.3 for why this distinction is
  load-bearing.

## 3. Current shape of the page (constraints the spec must respect)

- Rows are split into two independent tables by amount sign: `expenses`
  (`amount_minor_units < 0`) and `income` (`> 0`), both rendered from the same
  `{#snippet list(items)}`, so anything added to the snippet appears in both
  lists and must be parameterised by which list it is.
- `filteredTransactions` applies the category and source filters client-side;
  `sortTransactions` re-sorts per list.
- "All Time" pages in with `PAGE_SIZE = 300` via an `IntersectionObserver`;
  `transactions` grows as the user scrolls and `totalCount` (from
  `count_transactions`) is the true total, deliberately not
  `transactions.length`.
- A transfer is two rows sharing `transfer_group_id`: a negative leg (lands in
  **Expenses**) and a positive leg on the counterpart account (lands in
  **Income**). `delete_transaction` already deletes both legs whichever one is
  passed.
- Table currently has 6 columns; the empty state uses `colspan="6"`.

## 4. Behaviour

### 4.1 The checkbox column

- A new leading `<th>`/`<td>` column in the `list` snippet, holding one
  `<input type="checkbox">` per row. Table becomes 7 columns; the empty-state
  `colspan` becomes 7.
- **Hidden by default, revealed on hover**: `opacity: 0`, going to `1` on
  `tr:hover`. It must **also** be visible when `:checked` and on
  `:focus-visible`, otherwise a checked row looks unchecked once the pointer
  leaves it, and keyboard users can never see focus. Use `opacity`/
  `visibility`, never `display: none` — the column must keep its width so
  rows don't shift horizontally on hover.
- Each checkbox gets an accessible name, e.g.
  `aria-label="Select transaction {t.date} {t.source}"`.

### 4.2 The per-list header checkbox

- The header cell of the checkbox column holds a "select all" checkbox that is
  **hidden until at least one row in that same list is selected**, then
  behaves as:
  - unchecked/absent → not shown;
  - **indeterminate** when some but not all rows of that list are selected;
  - **checked** when every row of that list is selected;
  - clicking it when checked or indeterminate → clear that list's selection
    (which also hides it again); clicking when indeterminate could equally
    mean "select all" — pick **clear**, because it is the reversible option
    next to a delete button.
- Expenses and Income keep **separate** selections and separate header
  checkboxes. Selecting in one list never shows the other's header checkbox or
  its action menu.

### 4.3 What "select all" covers

"Select all" selects exactly the rows that list is rendering right now:
after `categoryFilter`/`sourceFilter`, and in "All Time" only the pages loaded
so far. It must **never** select rows the user cannot see, because the next
click may be Delete. Consequences:

- If the user then scrolls and another 300-row page loads, the new rows are
  **not** selected and the header checkbox drops from checked to
  indeterminate. That is correct and intended.
- The action menu shows the selected **count** so the user can tell
  "43 selected" apart from `totalCount` in the page header.
- Do not offer "select all N matching the filter" in this spec. It is a
  separate feature with a separate backend shape (delete-by-query), and is
  much easier to fire by accident.

### 4.4 The action menu

- Appears in the section header row, right of the `<h2>` ("Expenses" /
  "Income"), only for the list that has a non-empty selection. Disappears the
  moment that list's selection becomes empty.
- Contents: a count label (`{n} selected`), then
  - **trash icon** — delete the selected transactions;
  - **pen icon** — recategorize the selected transactions.
- Both use the existing icon-button styling. The delete control should reuse
  `DeleteButton` (`label="Delete N transactions"`), which already provides the
  click-outside/Escape-dismissable "Delete?" confirm step — a bulk delete
  should not be *less* guarded than the single-row one it replaces.
- The pen opens a `CategorySelect` populated with `categoryOptions` (same
  options as the per-row picker). Choosing an option applies immediately;
  dismissing without choosing does nothing.

### 4.5 Selection lifecycle

Selection is transient page state; nothing about it is persisted.

| Event | Effect on selection |
| --- | --- |
| Range mode change, custom date change, `load()` | cleared (both lists) |
| Category or source filter change | pruned to rows still visible (see below) |
| Sort change | unchanged (same rows, different order) |
| Another "All Time" page loads | unchanged; header checkbox recomputes |
| Row deleted (bulk or single) | its id dropped from both selections |
| Bulk recategorize succeeds | **cleared** for that list |
| Navigating away / remount | cleared (component state) |

Filter changes **prune** rather than preserve: a selected row that the filter
has hidden must not stay armed for deletion. Implement as: after
`filteredTransactions` recomputes, intersect each selection with the ids it
now contains.

### 4.6 Transfers in a bulk selection

This is the main correctness trap.

- Deleting either leg of a transfer deletes both (existing
  `TransactionService::delete_transaction`). So a bulk delete of a selection
  containing **both** legs must not fail or double-delete — the backend
  expands to transfer groups and de-duplicates before issuing deletes.
- Deleting an Expenses-list transfer leg silently removes a row from the
  **Income** list (and vice versa). After a bulk delete, remove the deleted
  ids **and any row sharing a `transfer_group_id` with them** from both
  `transactions` and both selections.
- The result toast must say so, matching the wording already used for the
  single-row case: e.g. `"7 transactions deleted (2 transfers removed on both
  accounts)."` when the selection contained transfer legs, otherwise
  `"7 transactions deleted."`.
- The command's return value should therefore carry the count actually
  deleted, not just `()` — the frontend cannot compute it (a selected leg may
  pull in a counterpart that was never loaded).

### 4.7 After a bulk action, do not call `load()`

`handleDelete` currently calls `load()`, which in "All Time" throws away every
page past the first. With bulk delete that is a much bigger regression (delete
3 rows after scrolling to 3000, get 300 back). Instead:

- **Delete**: splice the removed ids out of `transactions`, then
  `refreshCount()`. `nextOffset` is now larger than the rows held; that is
  acceptable — it can only cause a small gap at the seam, and the alternative
  (refetching every loaded page) is worse. Note this trade-off in the code
  comment.
- **Recategorize**: patch `category_id` on the affected rows in place, exactly
  as `handleCategoryChange` already does for one row. No refetch — the count
  is unchanged unless a category filter is active, in which case
  `refreshCount()` and let the prune in §4.5 drop rows that no longer match.
- On backend error: show `toast.error`, mutate **nothing** locally, keep the
  selection (the operation is atomic — see §5.3 — so the ledger is untouched
  and a retry is safe).

### 4.8 Accessibility / keyboard

- Every checkbox is reachable and toggleable by keyboard; hover-only reveal
  must not gate interaction (see §4.1).
- The action menu is a landmark that appears and disappears — give it
  `aria-live="polite"` or an `aria-label="Bulk actions for expenses"` region
  so the count change is announced.
- Shift-click range selection is **out of scope** for this spec.

## 5. Backend

### 5.1 Why not just loop the existing commands from the frontend

Looping `api.deleteTransaction(id)` over 300 ids means 300 IPC round-trips,
300 `MutexGuard` acquisitions and 300 separate implicit SQL transactions, and
a failure halfway through leaves a partial delete with no way to describe what
happened. Bulk gets its own path, all the way down.

### 5.2 New surface, layer by layer

`crates/domain/src/ports.rs` — two methods on `TransactionRepository`:

```rust
/// Deletes every listed transaction in one statement. Ids that no longer
/// exist are ignored rather than an error: a bulk delete races with nothing
/// else in a single-process app, but the caller expands transfer groups
/// first, so the same row can legitimately be named twice.
fn delete_many(&self, ids: &[TransactionId]) -> Result<(), RepositoryError>;

/// Recategorizes every listed transaction in one statement.
fn update_category_many(
    &self,
    ids: &[TransactionId],
    category_id: CategoryId,
) -> Result<(), RepositoryError>;
```

`crates/application/src/transaction_service.rs`:

```rust
/// Deletes the given transactions, expanding each transfer leg to its whole
/// group (see `delete_transaction`) and de-duplicating, so a selection
/// containing both legs deletes the pair once. Returns
/// `BulkDeleteOutcome { deleted, transfer_groups }` so the caller can
/// explain a count larger than the selection.
pub fn delete_transactions(&self, ids: &[TransactionId])
    -> Result<BulkDeleteOutcome, ApplicationError>;

/// Recategorizes the given transactions. The category is validated once,
/// not once per id — same check as `set_category`.
pub fn set_category_many(&self, ids: &[TransactionId], category_id: CategoryId)
    -> Result<u64, ApplicationError>;
```

`src-tauri/src/transactions.rs` — two commands, registered in
`src-tauri/src/lib.rs`:

```rust
#[tauri::command] pub fn delete_transactions(state, ids: Vec<String>)
    -> Result<BulkDeleteDto, String>;
#[tauri::command] pub fn set_transactions_category(state, ids: Vec<String>, category_id: String)
    -> Result<u64, String>;
```

`frontend/src/lib/api.ts` — one typed wrapper each (no `invoke()` from the
page):

```ts
deleteTransactions: (ids: string[]) =>
  invoke<BulkDeleteDto>("delete_transactions", { ids }),
setTransactionsCategory: (ids: string[], categoryId: string) =>
  invoke<number>("set_transactions_category", { ids, categoryId }),
```

`BulkDeleteDto = { deleted: number; transfer_groups: number }`.

### 5.3 Implementation notes for `infra-sqlite`

- **Chunk the ids.** SQLite's default `SQLITE_MAX_VARIABLE_NUMBER` is 999 in
  older builds; a 300-row page today, and a bigger one tomorrow, must not
  depend on that. Build `DELETE FROM transactions WHERE id IN (?,?,…)` in
  chunks of ≤ 500 bound parameters. Never interpolate ids into SQL text, even
  though they are UUIDs — they arrive as strings from the frontend.
- **Atomicity across chunks.** The repo holds `&Connection`, so wrap the
  chunk loop in `conn.unchecked_transaction()` and commit once; a bulk delete
  that half-succeeds is exactly what §4.7's "mutate nothing on error" relies
  on.
- **Empty id list is a no-op**, not a malformed `IN ()` — guard first.
- Reject the request in the command layer if `ids` is implausibly large
  (say > 10_000) rather than building a query from unbounded frontend input.

### 5.4 Domain invariants that must not be bent

- Deletes still refuse nothing and cascade nothing beyond the transfer pair —
  bulk delete is exactly N applications of the existing single-row rule, no
  new semantics.
- `set_category_many` validates the target category exists **before** writing
  anything (`ApplicationError::CategoryNotFound`), same as `set_category`.
- Adjustment (`role = "adjustment"`) and transfer rows stay selectable and
  deletable — they already are individually, and hiding them from bulk
  selection would be a surprising inconsistency. No change to reporting rules.

## 6. Tests

Match the per-layer conventions already in the repo.

**application** (`transaction_service.rs` `#[cfg(test)]`, fake repos):
- deletes every id in the list;
- a selection containing **both** legs of a transfer deletes the pair once and
  reports `deleted = 2, transfer_groups = 1`;
- a selection containing **one** leg deletes the counterpart too, so `deleted`
  exceeds `ids.len()`;
- empty id list is a no-op, `Ok`;
- `set_category_many` with an unknown category returns `CategoryNotFound` and
  writes nothing;
- `set_category_many` leaves ids not in the list untouched.
  The fake `TransactionRepository` needs the two new methods implemented.

**infra-sqlite** (real SQLCipher on a `tempfile` dir, per convention):
- `delete_many` over more ids than one chunk (e.g. 1_200) deletes all of them
  — the regression test for the variable-number limit;
- `delete_many` with a non-existent id mixed in still deletes the real ones;
- `update_category_many` updates only the listed rows;
- an error mid-way (e.g. category FK violation, if enforced) leaves the table
  unchanged — proves the wrapping transaction.

**frontend** (`npm run check` + `npm run build`, then manual per CLAUDE.md):
- verified in the real app (`npm run tauri dev`), not the browser preview, for
  anything touching data flow;
- browser preview still used to confirm the checkbox column, hover reveal,
  header checkbox states and the action menu render and theme correctly.

Manual checklist worth walking once: select in Expenses → menu appears only
there; select all → scroll to load another page → header goes indeterminate;
select a transfer leg → delete → counterpart disappears from the Income list
and the toast says both accounts; apply a source filter that hides a selected
row → selection count drops; delete in "All Time" after loading 3+ pages →
the already-loaded rows stay loaded.

## 7. Out of scope

- "Select all N matching the current filter" (server-side bulk by query).
- Shift-click range selection, `Cmd/Ctrl+A`.
- Bulk edit of anything other than category (date, account, source).
- Undo. There is none for the single-row delete either; adding it is its own
  feature.
- Any change to how transfers or adjustments are counted in reports.
