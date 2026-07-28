import { invoke } from "@tauri-apps/api/core";

export type AccountStatus = "active" | "archived";

export interface AccountDto {
  id: string;
  name: string;
  status: AccountStatus;
  opening_balance_minor_units: number;
  balance_minor_units: number;
  currency: string;
  source_patterns: string[];
}

export interface CategoryDto {
  id: string;
  name: string;
  parent_id: string | null;
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
  skipped_duplicates: number;
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
  archiveAccount: (id: string) => invoke<void>("archive_account", { id }),
  activateAccount: (id: string) => invoke<void>("activate_account", { id }),
  deleteAccount: (id: string) => invoke<void>("delete_account", { id }),

  listCategories: () => invoke<CategoryDto[]>("list_categories"),
  createCategory: (name: string, parentId: string | null) =>
    invoke<CategoryDto>("create_category", { name, parentId }),
  renameCategory: (id: string, name: string) =>
    invoke<void>("rename_category", { id, name }),
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
  deleteTransactionsInRange: (start: string, end: string) =>
    invoke<number>("delete_transactions_in_range", { start, end }),
  suggestAccountForSource: (source: string) =>
    invoke<string | null>("suggest_account_for_source", { source }),
  suggestCategoryForSource: (source: string) =>
    invoke<string | null>("suggest_category_for_source", { source }),

  previewCsvImport: (bytes: number[]) =>
    invoke<ImportPreviewDto>("preview_csv_import", { bytes }),
  commitCsvImport: (
    rows: {
      date: string;
      amount_minor_units: number;
      source: string;
      category: string | null;
    }[],
    categoryId: string | null,
    accountId: string,
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
};

/** Formats integer minor units (e.g. cents) as "12.34". */
export function formatMinorUnits(minorUnits: number): string {
  return (minorUnits / 100).toFixed(2);
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
  const byParent = new Map<string | null, CategoryDto[]>();
  for (const c of categories) {
    const key = c.parent_id;
    if (!byParent.has(key)) byParent.set(key, []);
    byParent.get(key)!.push(c);
  }
  const result: { id: string; label: string }[] = [];
  function walk(parentId: string | null, depth: number) {
    for (const c of byParent.get(parentId) ?? []) {
      result.push({ id: c.id, label: `${"— ".repeat(depth)}${c.name}` });
      walk(c.id, depth + 1);
    }
  }
  walk(null, 0);
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
