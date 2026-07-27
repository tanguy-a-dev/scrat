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
