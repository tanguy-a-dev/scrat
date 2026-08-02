import { invoke } from "@tauri-apps/api/core";

export interface AccountDto {
  id: string;
  name: string;
  opening_balance_minor_units: number;
  balance_minor_units: number;
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
  is_likely_balance_row: boolean;
  include_by_default: boolean;
  raw: string[];
}

export interface ImportPreviewDto {
  rows: ImportPreviewRowDto[];
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

  /** The `op://vault/item/field` pointer to the passphrase, or null when
   * 1Password unlock isn't set up. Readable while still locked. */
  get1PasswordReference: () =>
    invoke<string | null>("get_1password_reference"),
  /** Pass null to turn the integration off. */
  set1PasswordReference: (reference: string | null) =>
    invoke<void>("set_1password_reference", { reference }),
  /** Checks the referenced value actually opens this database. */
  test1PasswordReference: (reference: string) =>
    invoke<void>("test_1password_reference", { reference }),
  /** Fetches the passphrase from 1Password and unlocks in one Rust-side
   * step — the passphrase deliberately never crosses into JS. */
  unlockWith1Password: () => invoke<void>("unlock_with_1password"),

  listAccounts: () => invoke<AccountDto[]>("list_accounts"),
  createAccount: (name: string, openingBalanceMinorUnits: number) =>
    invoke<AccountDto>("create_account", {
      name,
      openingBalanceMinorUnits,
    }),
  renameAccount: (id: string, name: string) =>
    invoke<void>("rename_account", { id, name }),
  setOpeningBalance: (id: string, minorUnits: number) =>
    invoke<void>("set_opening_balance", { id, minorUnits }),
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
   * independently, each with its own category/description filters. */
  listTransactionsPage: (
    offset: number,
    limit: number,
    categoryId: string | null,
    descriptionContains: string | null,
    isIncome: boolean | null,
  ) =>
    invoke<TransactionDto[]>("list_transactions_page", {
      offset,
      limit,
      categoryId,
      descriptionContains,
      isIncome,
    }),
  countTransactions: (
    start: string,
    end: string,
    categoryId: string | null,
    descriptionContains: string | null,
    isIncome: boolean | null,
  ) =>
    invoke<number>("count_transactions", {
      start,
      end,
      categoryId,
      descriptionContains,
      isIncome,
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

  previewCsvImport: (bytes: number[]) =>
    invoke<ImportPreviewDto>("preview_csv_import", { bytes }),
  commitCsvImport: (
    rows: {
      date: string;
      amount_minor_units: number;
      description: string;
      category: string | null;
      subcategory: string | null;
    }[],
    categoryId: string | null,
    accountId: string | null,
  ) =>
    invoke<ImportSummaryDto>("commit_csv_import", {
      rows,
      categoryId,
      accountId,
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
