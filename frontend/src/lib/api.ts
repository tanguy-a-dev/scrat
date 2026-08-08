import { invoke } from "@tauri-apps/api/core";

import {
  monthNames,
  numberSeparators,
  shortMonthNames,
  t,
  type MessageKey,
} from "./i18n.svelte";

export interface AccountDto {
  id: string;
  name: string;
  balance_minor_units: number;
  /** False means nobody has told the app where this account started, so
   * `balance_minor_units` is the ledger sum alone — right only if the
   * account happened to begin at zero. Distinct from "started at zero",
   * which is an answer the user gave. */
  is_opening_balance_set: boolean;
  /** The anchor itself. Zero when unset — which is also what it contributes
   * to the balance — so read it alongside `is_opening_balance_set`, never on
   * its own. `balance_minor_units - opening_balance_minor_units` is the
   * ledger sum, which is what the starting-point preview works back from. */
  opening_balance_minor_units: number;
  has_transactions: boolean;
  currency: string;
  description_patterns: string[];
  is_default: boolean;
}

export interface CategoryDto {
  id: string;
  name: string;
  parent_id: string | null;
  /** Icon key, e.g. "house" — always set for a top-level category, always
   * null for a subcategory. See CATEGORY_ICONS in $lib/categoryIcons.ts. */
  icon: string | null;
  is_default: boolean;
}

/** What a transaction means, as opposed to which way its amount points.
 * Only "normal" is real income or spending: a transfer moves money between
 * two of the user's own accounts, and an adjustment corrects an account
 * whose statements can't be imported. Counting either one would inflate
 * both sides of every report — use `countsTowardTotals` rather than
 * comparing this by hand. */
export type TransactionRole = "normal" | "transfer" | "adjustment";

/** How the money moved — the instrument the bank named. A third axis,
 * independent of both the amount's sign and `TransactionRole`: it is purely
 * descriptive and never changes whether a row counts toward a total.
 *
 * In particular "bank_transfer" is NOT the "transfer" role. Rent paid by wire
 * is ordinary spending that happens to have been paid by wire; only money
 * moving between two of the user's own accounts is `role: "transfer"`. */
export type OperationKind =
  | "card"
  | "bank_transfer"
  | "direct_debit"
  | "check"
  | "cash"
  | "fees"
  | "other";

/** Display text for an operation kind. Falls back to the raw value rather
 * than to a guess, so a kind added on the Rust side before this map is
 * updated shows up as itself instead of silently reading as a card payment. */
export function operationKindLabel(kind: OperationKind | string): string {
  // A kind this build doesn't know (a database written by a newer version)
  // falls through to `t`'s own key fallback and shows the raw stored string,
  // which beats a blank cell.
  return t(`operationKind.${kind}` as MessageKey);
}

export interface TransactionDto {
  id: string;
  date: string;
  amount_minor_units: number;
  currency: string;
  description: string;
  category_id: string;
  account_id: string;
  role: TransactionRole;
  /** Shared by both legs of a transfer; null otherwise. Deleting either leg
   * deletes the other, so the two accounts can't drift apart. */
  transfer_group_id: string | null;
  operation_kind: OperationKind;
}

/** The filters `listTransactionsPage` and `countTransactions` both take —
 * bundled rather than passed as an ever-growing list of positional
 * arguments, mirroring `TransactionFilters` on the Rust side. Every field
 * `null` means "no filter", not "match nothing". */
export interface TransactionFilters {
  /** Matches the named category **and its subcategories**, not the named
   * category alone — see `TransactionFilters::category_id` on the Rust side,
   * which is what applies this for "All Time" and for every header count.
   * Anything filtering rows client-side has to resolve the branch the same
   * way (`buildCategoryBranches`), or the list and the count sitting above it
   * answer two different questions. */
  categoryId: string | null;
  descriptionContains: string | null;
  /** `true` narrows to positive amounts, `false` to negative, `null` to
   * both. */
  isIncome: boolean | null;
  accountId: string | null;
  operationKind: OperationKind | null;
  /** Inclusive bounds on the transaction's unsigned amount — expenses and
   * income are already split by `isIncome`, so "amount between X and Y"
   * means magnitude, not the signed minor units. */
  minAmountMinorUnits: number | null;
  maxAmountMinorUnits: number | null;
}

/** What `listTransactionsPage` orders a batch by — the exact strings
 * `TransactionSortField::parse` on the Rust side accepts, so no translation
 * table sits between the two and can drift out of sync. `category`/`account`
 * sort by the linked aggregate's name, not its id, and `operation_kind`
 * sorts by the same alphabetical-by-label order `operationKindLabel`
 * produces below, not by the raw stored string. */
export type TransactionSortField =
  | "date"
  | "amount"
  | "description"
  | "operation_kind"
  | "category"
  | "account";

export type SortDirection = "asc" | "desc";

/** Recognizes an imported row as money moving to another of the user's own
 * accounts. Matched as a case-insensitive substring of the row's description
 * text. */
export interface TransferRuleDto {
  id: string;
  pattern: string;
  counterpart_account_id: string;
}

/** Whether a transaction belongs in income/expense reporting. Account
 * balances are a different question — every role counts there, because the
 * money really did move. */
export function countsTowardTotals(t: { role: TransactionRole }): boolean {
  return t.role === "normal";
}

/** Result of a bulk delete. `deleted` can exceed the number of ids sent —
 * deleting one leg of a transfer always removes its counterpart too, even
 * if that counterpart was never part of the selection. */
export interface BulkDeleteDto {
  deleted: number;
  transfer_groups: number;
}

export interface ImportPreviewRowDto {
  date: string | null;
  amount_minor_units: number | null;
  description: string;
  csv_category: string | null;
  csv_subcategory: string | null;
  /** Read from the file's own operation-type column, or from the description
   * text when it has none. Always set — "card" when nothing said otherwise. */
  operation_kind: OperationKind;
  is_likely_balance_row: boolean;
  include_by_default: boolean;
  raw: string[];
}

/** One column of the file, as offered in the mapping editor. `header` is null
 * for a headerless export — `samples` is what makes such a column
 * recognizable. */
export interface ColumnSummaryDto {
  index: number;
  header: string | null;
  samples: string[];
}

/** Where amounts come from. A bank that splits money out and money in across
 * two columns needs `debit_credit`; reading only one half silently drops
 * every row on the other side. */
export type AmountSourceDto =
  | { kind: "single"; column: number }
  | { kind: "debit_credit"; debit: number; credit: number };

/** What each column of the file means. Detected on first preview, then
 * editable — send it back to `previewCsvImport` to re-read the file through
 * a corrected version. */
export interface ColumnMappingDto {
  has_header: boolean;
  column_count: number;
  date_column: number | null;
  /** A chrono pattern from `date_formats`; anything else is rejected by the
   * backend rather than handed to the date parser. */
  date_format: string;
  amount: AmountSourceDto | null;
  /** Columns joined, in order, to form each row's description. Always
   * explicit — there is no "everything unused" mode, because the columns a
   * bank puts around the description (instrument, category hint, reference,
   * flags) are text too, and sweeping them in buries the merchant name. */
  description_columns: number[];
  category_column: number | null;
  subcategory_column: number | null;
  currency_column: number | null;
  account_column: number | null;
  operation_kind_column: number | null;
}

export interface DateFormatOptionDto {
  pattern: string;
  label: string;
}

export interface ImportPreviewDto {
  rows: ImportPreviewRowDto[];
  /** How the rows above were read — the detector's guess on first preview,
   * or whatever mapping was last sent back. */
  mapping: ColumnMappingDto;
  columns: ColumnSummaryDto[];
  date_formats: DateFormatOptionDto[];
  /** Identifies this file's layout so the mapping can be remembered against
   * it. Send it back on commit. */
  signature: string;
  /** True when `mapping` was recalled from a previous import of the same
   * layout rather than detected. */
  remembered: boolean;
  /** Fraction of rows that yielded a usable date / non-zero amount. Measured
   * from the parsed result, so a mapping that produces nothing reports 0. */
  date_confidence: number;
  amount_confidence: number;
}

export interface ImportSummaryDto {
  imported: number;
  /** How many imported rows were recognized as transfers, and so also wrote
   * a mirrored leg on a counterpart account the user didn't import into. */
  mirrored: number;
}

/** A commitment inferred from the ledger — never stored, recomputed on every
 * call. See `crates/domain/src/recurring.rs` for the detection rules. */
export interface RecurringChargeDto {
  /** Raw description text of the most recent occurrence, noise included. */
  label: string;
  cadence: "weekly" | "monthly" | "quarterly" | "yearly";
  /** Positive magnitude — a recurring charge is always a cost. */
  typical_amount_minor_units: number;
  /** The same charge restated per month, so cadences can be summed. */
  monthly_equivalent_minor_units: number;
  currency: string;
  occurrences: number;
  first_seen: string;
  last_seen: string;
  next_expected: string;
  category_id: string;
  /** False once overdue by more than half a period — the "didn't I cancel
   * this?" case. Excluded from any committed-per-month total. */
  is_active: boolean;
}

export const api = {
  isDbInitialized: () => invoke<boolean>("is_db_initialized"),
  createDb: (passphrase: string) =>
    invoke<void>("create_db_with_passphrase", { passphrase }),
  unlockDb: (passphrase: string) => invoke<void>("unlock_db", { passphrase }),
  lockDb: () => invoke<void>("lock_db"),
  changePassphrase: (currentPassphrase: string, newPassphrase: string) =>
    invoke<void>("change_passphrase", {
      currentPassphrase,
      newPassphrase,
    }),

  listAccounts: () => invoke<AccountDto[]>("list_accounts"),
  createAccount: (name: string) => invoke<AccountDto>("create_account", { name }),
  renameAccount: (id: string, name: string) =>
    invoke<void>("rename_account", { id, name }),
  /** Sets the account's starting point by working backwards from a balance
   * the user can read off their bank: `opening = observed - ledger sum`.
   * Writes no transaction — unlike `reconcileAccount`, which posts a dated
   * adjustment for money that moved after the ledger begins.
   *
   * Also the edit path: it overwrites the anchor rather than refusing when
   * one already exists, so a mistyped starting point is correctable by
   * running it again. */
  establishOpeningBalance: (id: string, observedBalanceMinorUnits: number) =>
    invoke<void>("establish_opening_balance", { id, observedBalanceMinorUnits }),
  addDescriptionPattern: (id: string, pattern: string) =>
    invoke<void>("add_description_pattern", { id, pattern }),
  removeDescriptionPattern: (id: string, pattern: string) =>
    invoke<void>("remove_description_pattern", { id, pattern }),
  deleteAccount: (id: string) => invoke<void>("delete_account", { id }),
  setDefaultAccount: (id: string) => invoke<void>("set_default_account", { id }),

  listCategories: () => invoke<CategoryDto[]>("list_categories"),
  createCategory: (name: string, parentId: string | null) =>
    invoke<CategoryDto>("create_category", { name, parentId }),
  renameCategory: (id: string, name: string) =>
    invoke<void>("rename_category", { id, name }),
  setCategoryIcon: (id: string, icon: string) =>
    invoke<void>("set_category_icon", { id, icon }),
  moveCategory: (id: string, parentId: string | null) =>
    invoke<void>("move_category", { id, parentId }),
  deleteCategory: (id: string, reassignTo: string | null) =>
    invoke<void>("delete_category", { id, reassignTo }),
  /** The category the "Mean monthly spend" card treats as rent — `null` when
   * nothing is configured and no category is named "Rent" either. */
  getRentCategory: () => invoke<string | null>("get_rent_category"),
  setRentCategory: (id: string) => invoke<void>("set_rent_category", { id }),

  listTransactions: (start: string, end: string) =>
    invoke<TransactionDto[]>("list_transactions", { start, end }),
  /** One batch of the whole ledger in `sortField`/`sortDir` order, narrowed
   * by the same filters `countTransactions` takes — both the filtering and
   * the ordering are applied in the query, so a batch is a page of
   * *matching, correctly ordered* rows: sorting only the rows already
   * fetched would reorder just that page, not the whole matching set behind
   * it. `isIncome` (`true` for positive amounts, `false` for negative,
   * `null` for both) lets the Expenses and Income lists page through the
   * ledger independently, each with its own filters and sort. */
  listTransactionsPage: (
    offset: number,
    limit: number,
    filters: TransactionFilters,
    sortField: TransactionSortField,
    sortDir: SortDirection,
  ) =>
    invoke<TransactionDto[]>("list_transactions_page", {
      offset,
      limit,
      categoryId: filters.categoryId,
      descriptionContains: filters.descriptionContains,
      isIncome: filters.isIncome,
      accountId: filters.accountId,
      operationKind: filters.operationKind,
      minAmountMinorUnits: filters.minAmountMinorUnits,
      maxAmountMinorUnits: filters.maxAmountMinorUnits,
      sortField,
      sortDir,
    }),
  countTransactions: (start: string, end: string, filters: TransactionFilters) =>
    invoke<number>("count_transactions", {
      start,
      end,
      categoryId: filters.categoryId,
      descriptionContains: filters.descriptionContains,
      isIncome: filters.isIncome,
      accountId: filters.accountId,
      operationKind: filters.operationKind,
      minAmountMinorUnits: filters.minAmountMinorUnits,
      maxAmountMinorUnits: filters.maxAmountMinorUnits,
    }),
  createTransaction: (
    date: string,
    amountMinorUnits: number,
    description: string,
    categoryId: string,
    accountId: string,
  ) =>
    invoke<TransactionDto>("create_transaction", {
      date,
      amountMinorUnits,
      description,
      categoryId,
      accountId,
    }),
  deleteTransaction: (id: string) => invoke<void>("delete_transaction", { id }),
  /** Deletes every listed transaction, expanding any transfer leg to its
   * whole group — see `BulkDeleteDto`. `deleted` can exceed `ids.length`. */
  deleteTransactions: (ids: string[]) =>
    invoke<BulkDeleteDto>("delete_transactions", { ids }),
  setTransactionCategory: (id: string, categoryId: string) =>
    invoke<void>("set_transaction_category", { id, categoryId }),
  setTransactionsCategory: (ids: string[], categoryId: string) =>
    invoke<void>("set_transactions_category", { ids, categoryId }),
  suggestAccountForDescription: (description: string) =>
    invoke<string | null>("suggest_account_for_description", { description }),
  suggestCategoryForDescription: (description: string) =>
    invoke<string | null>("suggest_category_for_description", { description }),
  /** Writes one account's transactions to `destination`. Scoped to a single
   * account because that's the only scope that round-trips: import commits a
   * whole file to one account, and the duplicate check is per-account too. */
  exportTransactionsCsv: (accountId: string, destination: string) =>
    invoke<void>("export_transactions_csv", { accountId, destination }),
  listRecurringCharges: () => invoke<RecurringChargeDto[]>("list_recurring_charges"),
  /** Posts the difference between `observedBalanceMinorUnits` and what the
   * ledger says as a single adjustment. Resolves to null when the two
   * already agreed and nothing was written. */
  reconcileAccount: (
    accountId: string,
    observedBalanceMinorUnits: number,
    date: string,
  ) =>
    invoke<TransactionDto | null>("reconcile_account", {
      accountId,
      observedBalanceMinorUnits,
      date,
    }),

  listTransferRules: () => invoke<TransferRuleDto[]>("list_transfer_rules"),
  /** Converts transactions already in the ledger that match `accountId`'s
   * incoming-transfer rules, mirroring each onto that account — the
   * catch-up for a rule added after those rows were imported. */
  applyTransferRules: (accountId: string) =>
    invoke<{ converted: number }>("apply_transfer_rules", { accountId }),
  createTransferRule: (pattern: string, counterpartAccountId: string) =>
    invoke<TransferRuleDto>("create_transfer_rule", {
      pattern,
      counterpartAccountId,
    }),
  deleteTransferRule: (id: string) =>
    invoke<void>("delete_transfer_rule", { id }),

  /** Omit `mapping` to let the backend detect the columns; pass one to
   * re-read the same file through a user-corrected mapping. */
  previewCsvImport: (bytes: number[], mapping?: ColumnMappingDto) =>
    invoke<ImportPreviewDto>("preview_csv_import", { bytes, mapping: mapping ?? null }),
  /** Flags which of `rows` already sit in `accountId`'s ledger under the
   * same date, amount, and description — a hint the CSV import dialog uses
   * to default those rows unticked, not a constraint the ledger enforces.
   * `accountId` null falls back to the app default, same as
   * `commitCsvImport`. */
  checkDuplicateTransactions: (
    accountId: string | null,
    rows: { date: string; amount_minor_units: number; description: string }[],
  ) => invoke<boolean[]>("check_duplicate_transactions", { accountId, rows }),
  commitCsvImport: (
    rows: {
      date: string;
      amount_minor_units: number;
      description: string;
      category: string | null;
      subcategory: string | null;
      operation_kind: OperationKind;
    }[],
    categoryId: string | null,
    accountId: string | null,
    /** Pass both to remember this mapping for the next export with the same
     * layout. Omitting them imports without remembering anything. */
    signature?: string,
    mapping?: ColumnMappingDto,
    /** When true, a row is categorized from the most recent past transaction
     * with the same description first — falling back to the CSV's own
     * category (or the default) only when no such history exists. Reverses
     * the normal precedence, which trusts the CSV's category column first. */
    prioritizeHistoricalCategory = false,
    /** Whether past transactions are consulted at all to categorize a row.
     * On by default; turning it off ignores history entirely, so every row
     * falls back to the CSV's own category or the chosen default. */
    detectCategoryFromHistory = true,
  ) =>
    invoke<ImportSummaryDto>("commit_csv_import", {
      rows,
      categoryId,
      accountId,
      signature: signature ?? null,
      mapping: mapping ?? null,
      prioritizeHistoricalCategory,
      detectCategoryFromHistory,
    }),

  getCurrency: () => invoke<string>("get_currency"),
  getLanguage: () => invoke<string>("get_language"),
  /** Returns how many seeded categories were relabelled as a side effect —
   * the UI reports it, because a language change quietly renaming rows the
   * user can see should say so. */
  setLanguage: (language: string) => invoke<number>("set_language", { language }),
  setCurrency: (code: string) => invoke<void>("set_currency", { code }),
  getAutoLockMinutes: () => invoke<number>("get_auto_lock_minutes"),
  setAutoLockMinutes: (minutes: number) =>
    invoke<void>("set_auto_lock_minutes", { minutes }),
  exportDatabase: (destination: string) =>
    invoke<void>("export_database", { destination }),
  importDatabase: (source: string, passphrase: string) =>
    invoke<void>("import_database", { source, passphrase }),
  deleteDatabase: () => invoke<void>("delete_database"),
};

/** Formats integer minor units (e.g. cents) for an editable amount field —
 * "12.34" in English, "12,34" in French, ungrouped either way.
 *
 * Ungrouped because this is what a user types over: a thousands separator in
 * an input box is something they then have to delete. The decimal separator
 * still follows the language, because a French user shown "1234.56" reads a
 * malformed number, and `parseToMinorUnits` accepts either form back. */
export function formatMinorUnits(minorUnits: number): string {
  return (minorUnits / 100).toFixed(2).replace(".", numberSeparators().decimal);
}

const CURRENCY_SYMBOLS: Record<string, string> = {
  USD: "$",
  EUR: "€",
  GBP: "£",
  CAD: "$",
  AUD: "$",
  JPY: "¥",
};

/** Formats non-negative minor units in the current language's conventions —
 * "100,123.23" in English, "100 123,23" in French. */
export function formatMoney(minorUnits: number): string {
  const { group, decimal } = numberSeparators();
  const whole = Math.floor(minorUnits / 100);
  const cents = minorUnits % 100;
  const grouped = whole.toString().replace(/\B(?=(\d{3})+(?!\d))/g, group);
  return `${grouped}${decimal}${cents.toString().padStart(2, "0")}`;
}

/** Formats integer minor units with a leading currency symbol, e.g. "€100 123,23" or "-$5.00". */
export function formatCurrency(minorUnits: number, currencyCode: string): string {
  const symbol = CURRENCY_SYMBOLS[currencyCode] ?? `${currencyCode} `;
  const sign = minorUnits < 0 ? "-" : "";
  return `${sign}${symbol}${formatMoney(Math.abs(minorUnits))}`;
}

/** Same as `formatCurrency` but rounded to whole units, e.g. "€100 123".
 * For chart axis labels, where the cents are noise and the extra three
 * characters can overflow the label gutter. */
export function formatCurrencyRounded(minorUnits: number, currencyCode: string): string {
  const symbol = CURRENCY_SYMBOLS[currencyCode] ?? `${currencyCode} `;
  const whole = Math.round(minorUnits / 100);
  const sign = whole < 0 ? "-" : "";
  const grouped = Math.abs(whole)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, numberSeparators().group);
  return `${sign}${symbol}${grouped}`;
}

/** Parses a user-typed decimal amount into integer minor units.
 *
 * Accepts both "12.34" and "12,34" whatever the interface language, and
 * tolerates the spaces a user may have pasted in from a grouped figure.
 * Deliberately more forgiving than `formatMinorUnits` is strict: a French
 * user typing a comma on their numeric keypad was silently losing the
 * decimals to `parseFloat`, which stops at the first character it doesn't
 * recognise and returned 12 for "12,34".
 *
 * Still rejects anything with a second separator ("1,234.56"), because there
 * is no reading of that which is safe to guess at — one of the two is a
 * thousands separator and which one depends on a convention this function
 * cannot see. */
export function parseToMinorUnits(input: string): number | null {
  const cleaned = input.replace(/[\s\u00a0\u202f]/g, "");
  if ((cleaned.match(/[.,]/g) ?? []).length > 1) return null;
  const value = Number.parseFloat(cleaned.replace(",", "."));
  if (Number.isNaN(value)) return null;
  return Math.round(value * 100);
}

export interface CategoryOption {
  id: string;
  /** The two levels joined for display and for search — "Parent > Child",
   * or just the name for a top-level category. */
  label: string;
  /** The parent's name, when this is a subcategory. */
  parentName?: string;
  /** This category's own name, without its parent's. */
  name: string;
}

/** Builds a flat, depth-indented option list from a category tree.
 *
 * The two levels are carried alongside the joined `label` rather than left to
 * be recovered from it. Splitting `label` back on " > " looks equivalent and
 * isn't: `CategoryName` only rejects empty and over-long names, so a category
 * may legitimately *be called* "Sport > Fitness", and a renderer that split
 * the string would tear that one in half and attribute it to a parent that
 * doesn't exist. */
export function buildCategoryOptions(categories: CategoryDto[]): CategoryOption[] {
  const byId = new Map(categories.map((c) => [c.id, c]));
  const byParent = new Map<string | null, CategoryDto[]>();
  for (const c of categories) {
    const key = c.parent_id;
    if (!byParent.has(key)) byParent.set(key, []);
    byParent.get(key)!.push(c);
  }
  const result: CategoryOption[] = [];
  function walk(parentId: string | null) {
    for (const c of byParent.get(parentId) ?? []) {
      const parentName = c.parent_id ? byId.get(c.parent_id)?.name : undefined;
      const label = parentName ? `${parentName} > ${c.name}` : c.name;
      result.push({ id: c.id, label, parentName, name: c.name });
      walk(c.id);
    }
  }
  walk(null);
  return result;
}

/** For every category, the set of ids a filter naming it matches: itself plus
 * its subcategories.
 *
 * The client-side counterpart to the `category_id IN (SELECT id FROM
 * categories WHERE parent_id = ?)` branch in `list_page`/`count_in_range`.
 * Naming a parent has to mean the whole branch everywhere, not just in the
 * ranges that happen to be filtered by SQL: the Month/Year/Custom ranges fetch
 * unfiltered and narrow the rows here, while their header count comes from the
 * backend either way — so an exact-match predicate here showed "3 rows" under
 * a header reading "12 transactions", with the nine subcategory rows the user
 * clicked a parent to see missing.
 *
 * A whole map rather than a resolver called per row: the branch is the same
 * for every transaction being tested, and the categories it's built from
 * change only when the category list itself reloads.
 *
 * Built from the parent links alone, so a category naming a parent that isn't
 * in the list contributes only itself — the same thing SQL's subselect does
 * with a dangling reference, rather than dropping the row entirely. */
export function buildCategoryBranches(
  categories: CategoryDto[],
): Map<string, Set<string>> {
  const branches = new Map<string, Set<string>>();
  for (const c of categories) branches.set(c.id, new Set([c.id]));
  for (const c of categories) {
    if (c.parent_id) branches.get(c.parent_id)?.add(c.id);
  }
  return branches;
}

/** Whether `categoryId` is inside the branch a filter naming `filterId`
 * selects. Falls back to an exact match while the category list is still
 * empty (first render, before `listCategories` resolves) — the alternative,
 * treating an unknown filter as matching nothing, would blank a list that is
 * about to be correct. */
export function categoryMatchesFilter(
  branches: Map<string, Set<string>>,
  filterId: string,
  categoryId: string,
): boolean {
  const branch = branches.get(filterId);
  return branch ? branch.has(categoryId) : categoryId === filterId;
}

export type RangeMode = "month" | "year" | "all" | "custom";

/** Local-calendar YYYY-MM-DD. Deliberately not `toISOString()`, which converts
 * to UTC first and so rolls a local midnight back to the previous day anywhere
 * east of Greenwich — in Paris that made "this month" resolve to 31 Jul – 30
 * Aug and `todayIsoDate()` return yesterday. Transaction dates are plain
 * calendar days with no timezone of their own, so the only correct reading of
 * a `Date` here is its local components. */
function toIsoDate(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function todayIsoDate(): string {
  return toIsoDate(new Date());
}

/** A month-wide default is what makes opening "Set Dates" show a range with
 * transactions in it right away, instead of a same-day range that's empty
 * until the user has picked both ends themselves. */
export function oneMonthAgoIsoDate(): string {
  const now = new Date();
  return toIsoDate(new Date(now.getFullYear(), now.getMonth() - 1, now.getDate()));
}

/** Computes the [start, end] ISO date bounds for a range mode.
 *
 * `offset` steps whole periods away from the one containing today: -1 is last
 * month (or last year), +1 the next. It is the only way to name a period other
 * than the current one without falling back to `custom` and two hand-typed
 * dates. `all` has exactly one period and `custom`'s bounds are given
 * outright, so both ignore it.
 *
 * The stepping goes through the `Date` constructor's own month overflow
 * (month 12 is next January, month -1 last December) rather than arithmetic on
 * the components, so December → January needs no wrap case of its own. Day 0
 * of the following month is the last day of this one, which is likewise how
 * month lengths and leap years stay someone else's problem. */
export function computeRange(
  mode: RangeMode,
  opts?: { start?: string; end?: string; offset?: number },
): { start: string; end: string } {
  const now = new Date();
  const offset = opts?.offset ?? 0;
  if (mode === "month") {
    return {
      start: toIsoDate(new Date(now.getFullYear(), now.getMonth() + offset, 1)),
      end: toIsoDate(new Date(now.getFullYear(), now.getMonth() + offset + 1, 0)),
    };
  }
  if (mode === "year") {
    return {
      start: toIsoDate(new Date(now.getFullYear() + offset, 0, 1)),
      end: toIsoDate(new Date(now.getFullYear() + offset, 11, 31)),
    };
  }
  if (mode === "all") {
    return { start: "0001-01-01", end: "9999-12-31" };
  }
  return opts?.start && opts?.end
    ? { start: opts.start, end: opts.end }
    : { start: toIsoDate(now), end: toIsoDate(now) };
}

/** Human name for the period a mode and offset select — "August 2026", "2025",
 * "All time", "1–6 Aug 2026".
 *
 * Built from the mode and offset rather than by inspecting the computed
 * bounds: a month's label is "August 2026" whether or not today happens to
 * fall inside it, and reading that back out of two ISO strings would mean
 * re-deriving what `computeRange` already knew. `custom` is the exception —
 * there the bounds *are* the only description there is. */
export function describeRange(
  mode: RangeMode,
  offset: number,
  custom?: { start: string; end: string },
): string {
  const now = new Date();
  if (mode === "month") {
    const d = new Date(now.getFullYear(), now.getMonth() + offset, 1);
    return `${monthNames()[d.getMonth()]} ${d.getFullYear()}`;
  }
  if (mode === "year") return String(now.getFullYear() + offset);
  if (mode === "all") return t("range.allTime");
  if (!custom) return t("range.custom");
  return formatDateSpan(custom.start, custom.end);
}

/** The span of the same length immediately before `[start, end]` — the only
 * defensible "previous period" for a hand-picked range, which unlike a month
 * or a year has no calendar predecessor of its own. Comparing 30 days against
 * 30 days is a fair comparison; comparing them against a whole month is not,
 * which is why this matches the length rather than snapping to a boundary. */
export function precedingSpan(
  start: string,
  end: string,
): { start: string; end: string } {
  const s = new Date(`${start}T00:00:00`);
  const e = new Date(`${end}T00:00:00`);
  const days = Math.round((e.getTime() - s.getTime()) / 86400000) + 1;
  const bEnd = new Date(s);
  bEnd.setDate(bEnd.getDate() - 1);
  const bStart = new Date(bEnd);
  bStart.setDate(bStart.getDate() - (days - 1));
  return { start: toIsoDate(bStart), end: toIsoDate(bEnd) };
}

/** How many whole days a span covers, both ends included. Two periods of
 * different lengths can still be compared, but the difference has to be
 * visible when they are. */
export function spanDays(start: string, end: string): number {
  const s = new Date(`${start}T00:00:00`).getTime();
  const e = new Date(`${end}T00:00:00`).getTime();
  return Math.round((e - s) / 86400000) + 1;
}

/** A date span with the parts both ends share said once — "1–6 Aug 2026",
 * "28 Jul – 6 Aug 2026", "6 Dec 2025 – 6 Aug 2026". */
export function formatDateSpan(start: string, end: string): string {
  const a = new Date(`${start}T00:00:00`);
  const b = new Date(`${end}T00:00:00`);
  const shortMonth = (d: Date) => shortMonthNames()[d.getMonth()];
  if (a.getFullYear() === b.getFullYear()) {
    if (a.getMonth() === b.getMonth()) {
      return `${a.getDate()}–${b.getDate()} ${shortMonth(b)} ${b.getFullYear()}`;
    }
    return `${a.getDate()} ${shortMonth(a)} – ${b.getDate()} ${shortMonth(b)} ${b.getFullYear()}`;
  }
  return (
    `${a.getDate()} ${shortMonth(a)} ${a.getFullYear()} – ` +
    `${b.getDate()} ${shortMonth(b)} ${b.getFullYear()}`
  );
}
