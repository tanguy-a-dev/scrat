import { invoke } from "@tauri-apps/api/core";

export interface AccountDto {
  id: string;
  name: string;
  opening_balance_minor_units: number;
  balance_minor_units: number;
  currency: string;
  source_patterns: string[];
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

export interface TransactionDto {
  id: string;
  date: string;
  amount_minor_units: number;
  currency: string;
  source: string;
  category_id: string;
  account_id: string;
}

export interface ImportPreviewRowDto {
  date: string | null;
  amount_minor_units: number | null;
  source: string;
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
}

export const api = {
  isDbInitialized: () => invoke<boolean>("is_db_initialized"),
  createDb: (passphrase: string) =>
    invoke<void>("create_db_with_passphrase", { passphrase }),
  unlockDb: (passphrase: string) => invoke<void>("unlock_db", { passphrase }),

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
  addSourcePattern: (id: string, pattern: string) =>
    invoke<void>("add_source_pattern", { id, pattern }),
  removeSourcePattern: (id: string, pattern: string) =>
    invoke<void>("remove_source_pattern", { id, pattern }),
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
  createTransaction: (
    date: string,
    amountMinorUnits: number,
    source: string,
    categoryId: string,
    accountId: string,
  ) =>
    invoke<TransactionDto>("create_transaction", {
      date,
      amountMinorUnits,
      source,
      categoryId,
      accountId,
    }),
  deleteTransaction: (id: string) => invoke<void>("delete_transaction", { id }),
  setTransactionCategory: (id: string, categoryId: string) =>
    invoke<void>("set_transaction_category", { id, categoryId }),
  suggestAccountForSource: (source: string) =>
    invoke<string | null>("suggest_account_for_source", { source }),
  suggestCategoryForSource: (source: string) =>
    invoke<string | null>("suggest_category_for_source", { source }),
  exportTransactionsCsv: (destination: string) =>
    invoke<void>("export_transactions_csv", { destination }),

  previewCsvImport: (bytes: number[]) =>
    invoke<ImportPreviewDto>("preview_csv_import", { bytes }),
  commitCsvImport: (
    rows: {
      date: string;
      amount_minor_units: number;
      source: string;
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
      const suffix = c.is_default ? " (default)" : "";
      const parentName = c.parent_id ? byId.get(c.parent_id)?.name : undefined;
      const label = parentName ? `${parentName} > ${c.name}${suffix}` : `${c.name}${suffix}`;
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
