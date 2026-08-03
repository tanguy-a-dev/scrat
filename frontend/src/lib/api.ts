import { invoke } from "@tauri-apps/api/core";

export interface AccountDto {
  id: string;
  name: string;
  balance_minor_units: number;
  /** False means nobody has told the app where this account started, so
   * `balance_minor_units` is the ledger sum alone — right only if the
   * account happened to begin at zero. Distinct from "started at zero",
   * which is an answer the user gave. */
  is_opening_balance_set: boolean;
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
  switch (kind) {
    case "card":
      return "Card";
    case "bank_transfer":
      return "Transfer";
    case "direct_debit":
      return "Direct debit";
    case "check":
      return "Cheque";
    case "cash":
      return "Cash";
    case "fees":
      return "Fees";
    case "other":
      return "Other";
    default:
      return kind;
  }
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
   * adjustment for money that moved after the ledger begins. */
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

  listTransactions: (start: string, end: string) =>
    invoke<TransactionDto[]>("list_transactions", { start, end }),
  /** One newest-first batch of the whole ledger, narrowed by the same
   * filters `countTransactions` takes — the filters are applied in the query
   * so a batch is a page of *matching* rows, not a page of everything that
   * then has to be filtered down to almost nothing on this side. `isIncome`
   * (`true` for positive amounts, `false` for negative, `null` for both)
   * lets the Expenses and Income lists page through the ledger
   * independently, each with its own filters. */
  listTransactionsPage: (
    offset: number,
    limit: number,
    filters: TransactionFilters,
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
  exportTransactionsCsv: (destination: string) =>
    invoke<void>("export_transactions_csv", { destination }),
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
  ) =>
    invoke<ImportSummaryDto>("commit_csv_import", {
      rows,
      categoryId,
      accountId,
      signature: signature ?? null,
      mapping: mapping ?? null,
      prioritizeHistoricalCategory,
    }),

  getCurrency: () => invoke<string>("get_currency"),
  setCurrency: (code: string) => invoke<void>("set_currency", { code }),
  exportDatabase: (destination: string) =>
    invoke<void>("export_database", { destination }),
  importDatabase: (source: string, passphrase: string) =>
    invoke<void>("import_database", { source, passphrase }),
  deleteDatabase: () => invoke<void>("delete_database"),
};

/** Formats integer minor units (e.g. cents) as "12.34". */
export function formatMinorUnits(minorUnits: number): string {
  return (minorUnits / 100).toFixed(2);
}

const CURRENCY_SYMBOLS: Record<string, string> = {
  USD: "$",
  EUR: "€",
  GBP: "£",
  CAD: "$",
  AUD: "$",
  JPY: "¥",
};

/** Formats non-negative minor units with a space-grouped whole part and a comma decimal, e.g. "100 123,23". */
export function formatMoney(minorUnits: number): string {
  const whole = Math.floor(minorUnits / 100);
  const cents = minorUnits % 100;
  const grouped = whole.toString().replace(/\B(?=(\d{3})+(?!\d))/g, " ");
  return `${grouped},${cents.toString().padStart(2, "0")}`;
}

/** Formats integer minor units with a leading currency symbol, e.g. "€100 123,23" or "-$5,00". */
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
    .replace(/\B(?=(\d{3})+(?!\d))/g, " ");
  return `${sign}${symbol}${grouped}`;
}

/** Parses a user-typed decimal amount ("12.34") into integer minor units. */
export function parseToMinorUnits(input: string): number | null {
  const value = Number.parseFloat(input);
  if (Number.isNaN(value)) return null;
  return Math.round(value * 100);
}

/** Builds a flat, depth-indented option list from a category tree. */
export function buildCategoryOptions(
  categories: CategoryDto[],
): { id: string; label: string }[] {
  const byId = new Map(categories.map((c) => [c.id, c]));
  const byParent = new Map<string | null, CategoryDto[]>();
  for (const c of categories) {
    const key = c.parent_id;
    if (!byParent.has(key)) byParent.set(key, []);
    byParent.get(key)!.push(c);
  }
  const result: { id: string; label: string }[] = [];
  function walk(parentId: string | null) {
    for (const c of byParent.get(parentId) ?? []) {
      const parentName = c.parent_id ? byId.get(c.parent_id)?.name : undefined;
      const label = parentName ? `${parentName} > ${c.name}` : c.name;
      result.push({ id: c.id, label });
      walk(c.id);
    }
  }
  walk(null);
  return result;
}

export type RangeMode = "month" | "year" | "all" | "custom";

function toIsoDate(d: Date): string {
  return d.toISOString().slice(0, 10);
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

/** Computes the [start, end] ISO date bounds for a range mode. */
export function computeRange(
  mode: RangeMode,
  custom?: { start: string; end: string },
): { start: string; end: string } {
  const now = new Date();
  if (mode === "month") {
    return {
      start: toIsoDate(new Date(now.getFullYear(), now.getMonth(), 1)),
      end: toIsoDate(new Date(now.getFullYear(), now.getMonth() + 1, 0)),
    };
  }
  if (mode === "year") {
    return {
      start: toIsoDate(new Date(now.getFullYear(), 0, 1)),
      end: toIsoDate(new Date(now.getFullYear(), 11, 31)),
    };
  }
  if (mode === "all") {
    return { start: "0001-01-01", end: "9999-12-31" };
  }
  return custom ?? { start: toIsoDate(now), end: toIsoDate(now) };
}
