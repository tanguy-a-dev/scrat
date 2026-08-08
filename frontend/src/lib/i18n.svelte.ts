/* The app's translation layer.
 *
 * Hand-rolled, no dependency — the same call the hand-rolled donut chart got.
 * What a library would add here is a message *format* (ICU plurals, gender,
 * ordinals) and a loader; Scrat needs neither. Two languages, one plural rule
 * each, and every message shipped in the bundle because the app is offline by
 * construction and there is nothing to lazy-load from.
 *
 * ## How a message reaches the screen
 *
 * `t("nav.overview")` reads `language`, which is `$state`. Template
 * expressions in Svelte 5 track whatever `$state` they read, so every `t()`
 * call in markup re-runs when the language changes — no per-component
 * subscription, no key on the layout, no reload.
 *
 * ## Where the language lives
 *
 * In the encrypted database, alongside currency and auto-lock (see
 * `settings.language`). That has one consequence worth knowing: the
 * passphrase screen renders *before* any database is open, so it has no
 * language to read. `rememberLanguage`/`cachedLanguage` keep the last known
 * choice in `localStorage` purely so that screen can render in the language
 * the user last picked. It is a display hint and nothing else — the database
 * remains the only source of truth, and `setLanguage` overwrites the cache on
 * every load. It holds one of two fixed strings, no user data.
 */

/** Language tags, matching `Language::as_str()` in `crates/domain/src/language.rs`.
 * These cross the IPC boundary verbatim — `scrat-domain`'s
 * `language_spellings_match_the_frontend_union` test is the other half of
 * this contract. */
export type Language = "en" | "fr";

export const LANGUAGES: readonly Language[] = ["en", "fr"] as const;

/** What each language calls itself. Never translated: a user hunting for
 * their own language in a list they can't read needs to see "Français", not
 * "French" rendered in a language they don't speak. */
export const LANGUAGE_LABELS: Record<Language, string> = {
  en: "English",
  fr: "Français",
};

export const DEFAULT_LANGUAGE: Language = "en";

export function isLanguage(value: unknown): value is Language {
  return typeof value === "string" && (LANGUAGES as readonly string[]).includes(value);
}

const STORAGE_KEY = "scrat.language";

/** The last language the app was known to be in, for the pre-unlock screen.
 * Falls back to the app default when nothing is cached or storage is
 * unavailable (it is, in SSR and in tests). */
export function cachedLanguage(): Language {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return isLanguage(stored) ? stored : DEFAULT_LANGUAGE;
  } catch {
    return DEFAULT_LANGUAGE;
  }
}

function rememberLanguage(language: Language) {
  try {
    localStorage.setItem(STORAGE_KEY, language);
  } catch {
    // A cache that can't be written costs the lock screen its language and
    // nothing else. Never worth failing a language change over.
  }
}

/* ------------------------------------------------------------------ *
 * Messages
 * ------------------------------------------------------------------ */

/* Flat, dotted keys rather than nested objects: a key is greppable exactly as
   it appears in the markup, which is what you actually want when you find a
   stray English string in the UI and need to know whether it has a key yet.
   `{placeholders}` are interpolated by `t`.

   Keys ending `.one`/`.other` are plural forms — reach them through `tp`,
   never `t`. */
const en = {
  // ---- Navigation and chrome ----
  "nav.overview": "Overview",
  "nav.transactions": "Transactions",
  "nav.details": "Details",
  "nav.accounts": "Accounts",
  "nav.categories": "Categories",
  "nav.settings": "Settings",
  "nav.lock": "Lock",

  // ---- Shared vocabulary ----
  "common.save": "Save",
  "common.cancel": "Cancel",
  "common.delete": "Delete",
  "common.edit": "Edit",
  "common.rename": "Rename",
  "common.close": "Close",
  "common.confirm": "Confirm",
  "common.back": "Back",
  "common.next": "Next",
  "common.done": "Done",
  "common.loading": "Loading…",
  "common.search": "Search",
  "common.none": "None",
  "common.all": "All",
  "common.total": "Total",
  "common.income": "Income",
  "common.expenses": "Expenses",
  "common.balance": "Balance",
  "common.date": "Date",
  "common.amount": "Amount",
  "common.description": "Description",
  "common.category": "Category",
  "common.account": "Account",
  "common.subcategory": "Subcategory",
  "common.uncategorized": "Uncategorized",
  "common.optional": "optional",
  "common.yes": "Yes",
  "common.no": "No",

  // ---- CSV import dialog ----
  "import.title": "Import transactions from CSV",
  "import.dropzone": "CSV file drop zone",
  "import.dropHint": "or drop a file here, or paste ⌘V",
  "import.dialogTitle": "Import CSV",
  "import.fileTooLarge": "This file is {size} MB — too large to be a CSV export (limit is {limit} MB).",
  "import.mappingLooksWrong": "Only {dates}% of dates and {amounts}% of amounts could be read — the columns seem wrongly set.",
  "import.editColumns": "Edit columns",
  "import.savedBadge": "Saved",
  "import.savedBadgeTitle": "Reused from the last import of this file layout",
  "import.rereading": "Re-reading…",
  "import.notSet": "Not set",
  "import.dateFormat": "Date format",
  "import.moneyOut": "Money out (debit)",
  "import.moneyIn": "Money in (credit)",
  "import.moneyInHint": "Same column as money out when one signed column holds both.",
  "import.readFromDescription": "Read from the description",
  "import.firstRowHeader": "First row is a header",
  "import.categoriesSettings": "Categories settings",
  "import.defaultCategory": "Default category",
  "import.reusePastCategories": "Reuse categories from similar past transactions",
  "import.pastCategoriesOverride": "Let past categories override the file's category column",
  "import.uncategorizedDefault": "Uncategorized (default)",
  "import.uncategorizedDefaultPlaceholder": "Uncategorized (default)…",
  "import.destinationAccount": "Destination account (optional)…",
  "import.defaultAccount": "Default account",
  "import.accountIsDefault": "{name} (default)",
  "import.columnLabel": "Column {number}",
  "import.columnWithSamples": "{number} · {name} — {samples}",
  "import.columnWithName": "{number} · {name}",
  "import.importRow": "Import row {number}",
  "import.importRowWithDescription": "Import row {number}: {description}",
  "import.selectAllRows": "Select all rows",
  "import.rowsSelected": "{count} of {total} rows selected",
  "import.balanceLine": "balance line?",
  "import.alreadyInAccount": "already in this account — unticked",
  "import.showingCount": "· showing {count} — scroll for more",
  "import.importCount.one": "Import {count} transaction",
  "import.importCount.other": "Import {count} transactions",
  "import.mirroredSummary": "{imported} imported, {mirrored} recognized as transfers and mirrored onto the other account.",

  // ---- Transactions page ----
  "transactions.title": "Transactions",
  "transactions.count.one": "{count} transaction",
  "transactions.count.other": "{count} transactions",
  "transactions.none": "No transactions.",
  "transactions.addTransaction": "Add transaction",
  "transactions.importCsv": "Import CSV",
  "transactions.amountPlaceholder": "Amount (− expense / + income)",
  "transactions.categoryPlaceholder": "Category…",
  "transactions.accountPlaceholder": "Account…",
  "transactions.save": "Save transaction",
  "transactions.amountInvalid": "Amount must be a non-zero number (negative for expense, positive for income).",
  "transactions.chooseCategoryAndAccount": "Choose a category and an account.",
  "transactions.deleted": "Transaction deleted.",
  "transactions.transferDeleted": "Transfer deleted, on both accounts.",
  "transactions.bulkDeleted.one": "{count} transaction deleted.",
  "transactions.bulkDeleted.other": "{count} transactions deleted.",
  "transactions.bulkDeletedWithTransfers.one": "{count} transactions deleted ({groups} transfer removed on both accounts).",
  "transactions.bulkDeletedWithTransfers.other": "{count} transactions deleted ({groups} transfers removed on both accounts).",
  "transactions.unanchoredWarning": "Balances for {names} are off until you set a starting point — do it in Accounts.",
  "transactions.filterByAmount": "Filter by amount",
  "transactions.min": "Min",
  "transactions.max": "Max",
  "transactions.type": "Type",
  "transactions.allCategories": "All categories",
  "transactions.allAccounts": "All accounts",
  "transactions.allTypes": "All types",
  "transactions.filterByDescription": "Filter by description",
  "transactions.filterByType": "Filter by type",
  "transactions.filterByCategory": "Filter by category",
  "transactions.filterByAccount": "Filter by account",
  "transactions.searchDescription": "Search description…",
  "transactions.searchType": "Search type…",
  "transactions.transferBadge": "transfer",
  "transactions.transferBadgeTitle": "Between your own accounts — not counted as spending",
  "transactions.adjustmentBadge": "adjustment",
  "transactions.adjustmentBadgeTitle": "Balance adjustment, posted from Accounts — not counted as spending",
  "transactions.deleteTransaction": "Delete transaction",
  "transactions.selectTransaction": "Select transaction {date} {description}",
  "transactions.showExpenses": "Show expenses",
  "transactions.hideExpenses": "Hide expenses",
  "transactions.showIncome": "Show income",
  "transactions.hideIncome": "Hide income",
  "transactions.resetExpenses": "Reset expenses sort and filters",
  "transactions.resetIncome": "Reset income sort and filters",
  "transactions.resetTitle": "Reset sort and filters",
  "transactions.reset": "Reset",
  "transactions.bulkExpenses": "Bulk actions for expenses",
  "transactions.bulkIncome": "Bulk actions for income",
  "transactions.recategorizeExpenses": "Recategorize selected expenses",
  "transactions.recategorizeIncome": "Recategorize selected income",
  "transactions.selectedCount": "{count} selected",
  "transactions.deleteSelected": "Delete {count} transactions",
  "transactions.loadingMore": "Loading more…",
  "transactions.allLoaded": "All transactions loaded.",
  "transactions.scrollToTop": "Scroll to top",

  // ---- Details page ----
  "details.title": "Details",
  "details.month": "Month",
  "details.year": "Year",
  "details.allTime": "All Time",
  "details.setDates": "Set Dates",
  "details.compare": "Compare",
  "details.compareTitle": "Compare this period against another",
  "details.compareDisabled": "All Time is a single period — there is nothing to compare it against",
  "details.previousPeriod": "Previous {period}",
  "details.previousPeriodKey": "Previous {period} (←)",
  "details.nextPeriod": "Next {period}",
  "details.nextPeriodKey": "Next {period} (→)",
  "details.previousComparison": "Previous comparison {period}",
  "details.nextComparison": "Next comparison {period}",
  "details.period.month": "month",
  "details.period.year": "year",
  "details.daysSoFar": "{elapsed} of {total} days so far",
  "details.precedingSpan": "Preceding span",
  "details.lengthMismatch": "{daysA} days vs {daysB} — not the same length",
  "details.viewTransactions": "View {name} transactions",
  "details.show": "Show {name}",
  "details.hide": "Hide {name}",
  "details.noChange": "no change",
  "details.shareOfPanel": "{period} — share of {panel}",
  "details.shareOfSlice": "Share of {name}",
  "details.shareOfTotal": "Share of total {panel}",
  "details.leftThisPeriod": "Left this period",
  "details.was": "was {amount}",
  "details.latestMonth": "This month is the latest there is",
  "details.latestYear": "This year is the latest there is",
  "details.vs": "vs",

  // ---- Overview page ----
  "overview.title": "Overview",
  // Split around the link rather than interpolating one: `t` returns text,
  // which Svelte escapes, so an <a> in the message would render as literal
  // markup. The two halves let the anchor stay in the markup where it belongs.
  "overview.noAccountsBefore": "No accounts yet. Head to",
  "overview.noAccountsAfter": "to add one.",
  "overview.totalAvailable": "Total available",
  "overview.noMovement": "No movement this month",
  "overview.movementThisMonth": "{amount} this month",
  "overview.thisMonth": "This month",
  "overview.spentSoFar": "Spent so far",
  "overview.sinceTheFirst": "Since the 1st",
  "overview.vsLastMonth": "vs. same point last month",
  "overview.nothingSpentEither": "Nothing spent in either month yet",
  "overview.more": "more",
  "overview.less": "less",
  "overview.identical": "identical",
  "overview.lastMonthByDay": "{amount} by day {day} last month",
  "overview.meanMonthlySpend": "Mean monthly spend",
  "overview.meanSpendOptions": "Mean monthly spend options",
  "overview.editRentCategory": "Edit rent category",
  "overview.withRent": "With rent",
  "overview.withoutRent": "Without rent",
  "overview.overLastMonths": "Over the last {months} months",
  "overview.meanMonthlySavings": "Mean monthly savings",
  "overview.incomeExpensesByMonth": "Income & expenses by month",
  "overview.savings": "Savings",
  "overview.noTransactionsInMonths": "No transactions in the last {months} months.",
  "overview.balanceOverTime": "Balance over time",
  "overview.overMonths": "over {months} months",
  "overview.notEnoughHistory": "Not enough history yet — balance over time needs at least two months.",
  "overview.monthIncome": "{month} income: {amount}",
  "overview.monthExpenses": "{month} expenses: {amount}",
  "overview.monthSavings": "{month} savings: {amount}",
  "overview.endOfMonth": "End of {month}: {amount}",
  "overview.recurring": "Recurring commitments",
  "overview.perMonthAcross.one": "{amount} / month across {count} charge",
  "overview.perMonthAcross.other": "{amount} / month across {count} charges",
  "overview.noRecurring": "Nothing detected yet. A charge has to appear at least three times, on a steady rhythm and for about the same amount, before it counts as recurring.",
  "overview.nextOn": "Next {date}",
  "overview.perMonthNext": "≈ {amount}/mo · next {date}",
  "overview.showFewer": "Show fewer",
  "overview.showAllRecurring": "Show all {count} recurring charges",
  "overview.notSeenRecently": "Not seen recently",
  "overview.lapsedHint": "These billed on a rhythm and then stopped. Either they were cancelled, or a payment failed and is worth checking.",
  "overview.lastSeen": "Last seen {date}",
  "operationKind.card": "Card",
  "operationKind.bank_transfer": "Transfer",
  "operationKind.direct_debit": "Direct debit",
  "operationKind.check": "Cheque",
  "operationKind.cash": "Cash",
  "operationKind.fees": "Fees",
  "operationKind.other": "Other",
  "cadence.weekly": "weekly",
  "cadence.monthly": "monthly",
  "cadence.quarterly": "quarterly",
  "cadence.yearly": "yearly",

  // ---- Accounts page ----
  "accounts.title": "Accounts",
  "accounts.dragToReorder": "Drag to reorder",
  "accounts.namePlaceholder": "Account name",
  "accounts.addAccount": "Add account",
  "accounts.empty": "No accounts yet — add one above.",
  "accounts.balanceLabel": "balance: {amount}",
  "accounts.default": "default",
  "accounts.setAsDefault": "Set as default",
  "accounts.editStartingPoint": "Edit starting point",
  "accounts.setStartingPoint": "Set starting point",
  "accounts.addAdjustment": "Add adjustment",
  "accounts.deleteAccount": "Delete account",
  "accounts.deleted": "\u201c{name}\u201d deleted.",
  "accounts.balanceNotANumber": "Balance must be a number.",
  "accounts.startingPointUpdated": "Starting point updated for \u201c{name}\u201d.",
  "accounts.startingPointSet": "Starting point set for \u201c{name}\u201d.",
  "accounts.unanchored": "Starting point not set — this balance is only the transactions on record, so it's off by whatever the account held before them.",
  "accounts.noLedgerEntry": "— no entry is added to the ledger",
  "accounts.bankBalanceToday": "Balance your bank shows today",
  "accounts.transactionsOnRecord": "Transactions on record",
  "accounts.startingPointNow": "Starting point now",
  "accounts.startingPointBecomes": "Starting point becomes",
  "accounts.anchorHint": "Use this when the balance is wrong all the way back. Works out what the account held before your earliest recorded transaction, correcting every past balance at once.",
  "accounts.anchorHintReplaces": "This replaces the starting point outright — but it won't undo an adjustment posted by mistake, only absorb it. Delete that entry from Transactions first if there is one.",
  "accounts.oneEntryDatedToday": "— one entry, dated today",
  "accounts.appCurrentlyShows": "App currently shows",
  "accounts.adjustmentPosted": "Adjustment posted",
  "accounts.adjustmentNone": "none — already matches",
  "accounts.adjustedBy": "Adjusted by {amount}.",
  "accounts.alreadyMatched": "\u201c{name}\u201d already matched — nothing to adjust.",
  "accounts.reconcileHint": "Use this when money moved that you never imported — fees, interest, market movement. Past balances are left as they were, and the adjustment doesn't count as spending.",
  "accounts.belongsToThisAccount": "Belongs to this account",
  "accounts.belongsTitle": "Matched against an imported row's description text to decide which account it belongs to",
  "accounts.removePattern": "Remove pattern",
  "accounts.addPatternPlaceholder": "Add description pattern…",
  "accounts.transfersInto": "Transfers into this account",
  "accounts.transfersIntoTitle": "An imported row matching one of these is money you sent to this account — it's mirrored here automatically, and left out of spending totals",
  "accounts.removeTransferRule": "Remove transfer rule",
  "accounts.addTransferPatternPlaceholder": "Add transfer pattern…",
  "accounts.applyToPast": "Apply to past transactions",
  "accounts.applyRulesHint": "Rescans every transaction already in the ledger against this account's transfer patterns above, converting any match into a transfer pair — the same thing a new import would have done, for rows imported before the pattern existed.",
  "accounts.converted.one": "{count} existing transaction converted to a transfer.",
  "accounts.converted.other": "{count} existing transactions converted to transfers.",
  "accounts.noMatchesFound": "No matching transactions found.",

  // ---- Categories page ----
  "categories.title": "Categories",
  "categories.summary": "{categories} categories · {subcategories} subcategories",
  "categories.newNamePlaceholder": "New category name",
  "categories.addCategory": "Add category",
  "categories.empty": "No categories yet — add one above.",
  "categories.deleted": "\u201c{name}\u201d deleted.",
  "categories.reassignDialogLabel": "Reassign transactions before deleting",
  "categories.reassignPrompt": "\u201c{name}\u201d still has transactions. Choose a category to move them to before deleting:",
  "categories.selectCategory": "Select a category…",
  "categories.searchCategory": "Search category…",
  "categories.reassignAndDelete": "Reassign & delete",

  // ---- Shared components ----
  "component.deleteConfirm": "Delete?",
  "component.confirmDelete": "Confirm delete",
  "component.previous": "Previous",
  "component.next": "Next",
  "component.previousMonth": "Previous month",
  "component.nextMonth": "Next month",
  "component.byDate": "By date",
  "component.byMonth": "By month",
  "component.dismiss": "Dismiss",
  "component.select": "Select…",
  "component.searchPlaceholder": "Search…",
  "component.noMatches": "No matches.",
  "find.placeholder": "Find on page…",
  "find.unsupported": "unsupported",
  "find.noResults": "No results",
  "find.previousMatch": "Previous match",
  "find.nextMatch": "Next match",
  "find.close": "Close find bar",
  "categoryCard.changeIcon": "Change icon",
  "categoryCard.categoryName": "Category name",
  "categoryCard.subcategoryName": "Subcategory name",
  "categoryCard.deleteCategory": "Delete category",
  "categoryCard.deleteSubcategory": "Delete subcategory",
  "categoryCard.addSubcategory": "Add subcategory",
  "categoryCard.subcategoryPlaceholder": "Subcategory",

  // ---- Command palette ----
  "palette.placeholder": "Type a command or search…",
  "palette.noMatches": "No matching commands.",
  "palette.navigate": "Navigate",
  "palette.actions": "Actions",
  "palette.nextPage": "Next page",
  "palette.previousPage": "Previous page",
  "palette.goTo": "Go to {page}",
  "palette.addTransaction": "Add transaction",
  "palette.importCsv": "Import CSV",

  // ---- Unlock / create ----
  "unlock.tagline": "Set a passphrase to encrypt your local data. There is no recovery — if you lose it, your data is unreadable.",
  "unlock.enterPassphrase": "Enter your passphrase to unlock your data.",
  "unlock.passphrase": "Passphrase",
  "unlock.confirmPassphrase": "Confirm passphrase",
  "unlock.create": "Create encrypted database",
  "unlock.unlock": "Unlock",
  "unlock.tooShort": "Passphrase must be at least {min} characters.",
  "unlock.mismatch": "Passphrases do not match.",
  "unlock.empty": "Passphrase cannot be empty.",

  // ---- Date ranges ----
  "range.thisMonth": "This month",
  "range.thisYear": "This year",
  "range.allTime": "All time",
  "range.custom": "Custom",
  "range.setDates": "Set dates",
  "range.from": "From",
  "range.to": "To",
  "range.apply": "Apply",

  // ---- Settings ----
  "settings.title": "Settings",
  "settings.language": "Language",
  "settings.languageHelp": "Changes the interface language. Default categories you haven't renamed are relabelled too; anything you renamed is left alone.",
  "settings.languageSaved": "Language set to {language}.",
  "settings.categoriesRelabelled.one": "{count} default category was relabelled.",
  "settings.categoriesRelabelled.other": "{count} default categories were relabelled.",
  "settings.currency": "Currency",
  "settings.currencySaved": "Currency set to {code}.",
  "settings.autoLock": "Auto-lock",
  "settings.autoLockSaved": "Auto-lock set to {label}.",
  "settings.autoLock.never": "Never",
  "settings.autoLock.oneMinute": "1 minute",
  "settings.autoLock.tenMinutes": "10 minutes",
  "settings.autoLock.oneHour": "1 hour",
  "settings.currencyHint": "Relabels amounts only — past transactions aren't converted.",
  "settings.autoLockHint": "Locks the app and asks for your passphrase again after this much time without mouse or keyboard activity.",
  "settings.passphrase": "Passphrase",
  "settings.passphraseHint": "Change the passphrase used to encrypt your database.",
  "settings.changePassphrase": "Change passphrase",
  "settings.passphraseNoRecovery": "No recovery — if you lose the new passphrase, your data is unreadable.",
  "settings.currentPassphrase": "Current passphrase",
  "settings.newPassphrase": "New passphrase",
  "settings.confirmNewPassphrase": "Confirm new passphrase",
  "settings.passphraseMinimum": "At least {min} characters.",
  "settings.passphraseTooShort": "New passphrase must be at least {min} characters.",
  "settings.passphraseMismatch": "New passphrases do not match.",
  "settings.passphraseChanged": "Passphrase changed.",
  "settings.changing": "Changing…",
  "settings.exportDatabase": "Export database",
  "settings.exportDatabaseHint": "Saves an encrypted copy of your database file.",
  "settings.export": "Export",
  "settings.exporting": "Exporting…",
  "settings.exportedTo": "Exported to {path}",
  "settings.exportCsv": "Export transaction CSV",
  "settings.exportCsvHint": "Saves one account's transactions as a CSV file, readable outside Scrat.",
  "settings.exportCsvButton": "Export CSV",
  "settings.noAccountsToExport": "Add an account first — there's nothing to export yet.",
  "settings.chooseAccount": "Choose an account…",
  "settings.searchAccount": "Search account…",
  "settings.importDatabase": "Import database",
  "settings.importDatabaseHint": "Replaces everything in Scrat with another encrypted database file.",
  "settings.chooseFileToImport": "Choose file to import",
  "settings.importWarningStrong": "This will permanently replace your current database",
  "settings.importWarningRest": "with {file}. This cannot be undone — export your current database first if you want to keep a copy.",
  "settings.importPassphrase": "Passphrase for the imported file",
  "settings.replaceDatabase": "Replace database",
  "settings.importing": "Importing…",
  "settings.databaseImported": "Database imported.",
  "settings.contactTitle": "Report a bug or contact the maintainer",
  "settings.contactHint": "Found a bug, or have a question? Write to",
  "settings.sendEmail": "Send an email",
  "settings.mailAppFailed": "Couldn't open your mail app — write to {address}.",
  "settings.deleteTitle": "Delete my data",
  "settings.deleteHint": "Permanently deletes your local database. No backup is made.",
  "settings.deleteWarningStrong": "This will permanently delete all of your data.",
  "settings.deleteWarningRest": "There is no undo and no backup. Type {word} below to confirm.",
  "settings.deleteConfirmLabel": "Type {word} to confirm",
  "settings.deletePermanently": "Permanently delete",
  "settings.deleting": "Deleting…",
  "settings.dataDeleted": "Your data has been deleted.",

  // ---- Errors from the backend, keyed by `codes::` in src-tauri/src/errors.rs ----
  "error.db_locked": "The database is locked. Unlock it and try again.",
  "error.db_already_exists": "A database already exists on this machine.",
  "error.incorrect_passphrase": "Incorrect passphrase.",
  "error.passphrase_empty": "The passphrase cannot be empty.",
  "error.passphrase_too_short": "The passphrase must be at least {min} characters.",
  "error.app_data_dir_unavailable": "Could not find where to store your data: {detail}",
  "error.database_error": "Database error: {detail}",
  "error.filesystem_error": "File error: {detail}",
  "error.account_not_found": "That account no longer exists.",
  "error.category_not_found": "That category no longer exists.",
  "error.account_name_empty": "An account needs a name.",
  "error.account_name_too_long": "An account name cannot be longer than {max} characters.",
  "error.account_pattern_empty": "The pattern cannot be empty.",
  "error.category_name_empty": "A category needs a name.",
  "error.category_name_too_long": "A category name cannot be longer than {max} characters.",
  "error.category_self_parent": "A category cannot be its own parent.",
  "error.category_unknown_icon": "'{icon}' is not an icon this version knows.",
  "error.subcategory_cannot_have_icon": "A subcategory cannot have its own icon.",
  "error.category_seed_key_empty": "That category's built-in identifier is invalid.",
  "error.amount_zero": "The amount cannot be zero.",
  "error.description_empty": "A description is required.",
  "error.description_too_long": "A description cannot be longer than {max} characters.",
  "error.transfer_without_group": "A transfer must have a counterpart.",
  "error.group_without_transfer_role": "Only a transfer can belong to a transfer pair.",
  "error.unknown_transaction_role": "'{value}' is not a transaction type this version knows.",
  "error.unknown_operation_kind": "'{value}' is not a payment method this version knows.",
  "error.invalid_currency_code": "'{value}' is not a valid currency code.",
  "error.currency_mismatch": "Cannot combine amounts in {left} and {right}.",
  "error.invalid_id": "'{value}' is not a valid identifier.",
  "error.invalid_date": "'{value}' is not a valid date.",
  "error.account_has_transactions": "This account still has {count} transaction(s). Reassign or delete them first.",
  "error.category_requires_reassignment": "This category still has {count} transaction(s). Choose a category to move them to.",
  "error.default_category_protected": "The default category cannot be renamed or deleted.",
  "error.parent_is_subcategory": "A subcategory cannot itself hold subcategories.",
  "error.category_has_subcategories": "This category has subcategories of its own, so it cannot become one.",
  "error.duplicate_transfer_rule": "A transfer rule for {pattern} already exists.",
  "error.balance_out_of_range": "That balance is too large to work with.",
  "error.invalid_reorder": "Couldn't reorder accounts — try reloading the page.",
  "error.unsupported_language": "'{value}' is not a language this version speaks.",
  "error.auto_lock_invalid": "Auto-lock must be never, 1, 10, or 60 minutes.",
  "error.auto_lock_stored_invalid": "The stored auto-lock setting is invalid.",
  "error.nothing_to_export": "There is no database to export yet.",
  "error.import_file_missing": "The selected file does not exist.",
  "error.import_finalize_failed": "Could not finish the restore ({detail}). Your original database was not modified — reload the app and unlock it with your original passphrase.",
  "error.import_reopen_failed": "The database was replaced but could not be reopened ({detail}). Reload the app and unlock it with the restored file's passphrase.",
  "error.csv_file_too_large": "This file is {size_mb} MB — too large to be a CSV export (limit is {limit_mb} MB).",
  "error.no_destination_account": "No destination account chosen, and no default is set — pick one, or set a default in Accounts.",
  "error.too_many_selected": "Too many transactions selected (at most {max}).",
  "error.unknown": "Something went wrong ({code}).",
} as const;

export type MessageKey = keyof typeof en;

/* `Record<MessageKey, string>` is the whole enforcement mechanism: a key
   added to `en` and forgotten here fails `npm run check`, rather than
   reaching a French user as an English sentence. */
const fr: Record<MessageKey, string> = {
  // ---- Navigation and chrome ----
  "nav.overview": "Vue d'ensemble",
  "nav.transactions": "Transactions",
  "nav.details": "Détails",
  "nav.accounts": "Comptes",
  "nav.categories": "Catégories",
  "nav.settings": "Réglages",
  "nav.lock": "Verrouiller",

  // ---- Shared vocabulary ----
  "common.save": "Enregistrer",
  "common.cancel": "Annuler",
  "common.delete": "Supprimer",
  "common.edit": "Modifier",
  "common.rename": "Renommer",
  "common.close": "Fermer",
  "common.confirm": "Confirmer",
  "common.back": "Retour",
  "common.next": "Suivant",
  "common.done": "Terminé",
  "common.loading": "Chargement…",
  "common.search": "Rechercher",
  "common.none": "Aucune",
  "common.all": "Tout",
  "common.total": "Total",
  "common.income": "Revenus",
  "common.expenses": "Dépenses",
  "common.balance": "Solde",
  "common.date": "Date",
  "common.amount": "Montant",
  "common.description": "Libellé",
  "common.category": "Catégorie",
  "common.account": "Compte",
  "common.subcategory": "Sous-catégorie",
  "common.uncategorized": "Non classé",
  "common.optional": "facultatif",
  "common.yes": "Oui",
  "common.no": "Non",

  // ---- CSV import dialog ----
  "import.title": "Importer des transactions depuis un CSV",
  "import.dropzone": "Zone de dépôt du fichier CSV",
  "import.dropHint": "ou déposez un fichier ici, ou collez ⌘V",
  "import.dialogTitle": "Import CSV",
  "import.fileTooLarge": "Ce fichier fait {size} Mo — trop volumineux pour un export CSV (limite : {limit} Mo).",
  "import.mappingLooksWrong": "Seulement {dates} % des dates et {amounts} % des montants ont pu être lus — les colonnes semblent mal réglées.",
  "import.editColumns": "Modifier les colonnes",
  "import.savedBadge": "Enregistré",
  "import.savedBadgeTitle": "Repris du dernier import de cette structure de fichier",
  "import.rereading": "Relecture…",
  "import.notSet": "Non défini",
  "import.dateFormat": "Format de date",
  "import.moneyOut": "Débit",
  "import.moneyIn": "Crédit",
  "import.moneyInHint": "Même colonne que le débit lorsqu'une seule colonne signée contient les deux.",
  "import.readFromDescription": "Lire depuis le libellé",
  "import.firstRowHeader": "La première ligne est un en-tête",
  "import.categoriesSettings": "Réglages des catégories",
  "import.defaultCategory": "Catégorie par défaut",
  "import.reusePastCategories": "Réutiliser les catégories de transactions passées similaires",
  "import.pastCategoriesOverride": "Laisser les catégories passées primer sur la colonne catégorie du fichier",
  "import.uncategorizedDefault": "Non classé (par défaut)",
  "import.uncategorizedDefaultPlaceholder": "Non classé (par défaut)…",
  "import.destinationAccount": "Compte de destination (facultatif)…",
  "import.defaultAccount": "Compte par défaut",
  "import.accountIsDefault": "{name} (par défaut)",
  "import.columnLabel": "Colonne {number}",
  "import.columnWithSamples": "{number} · {name} — {samples}",
  "import.columnWithName": "{number} · {name}",
  "import.importRow": "Importer la ligne {number}",
  "import.importRowWithDescription": "Importer la ligne {number} : {description}",
  "import.selectAllRows": "Tout sélectionner",
  "import.rowsSelected": "{count} lignes sur {total} sélectionnées",
  "import.balanceLine": "ligne de solde ?",
  "import.alreadyInAccount": "déjà présente sur ce compte — décochée",
  "import.showingCount": "· {count} affichées — faites défiler pour voir plus",
  "import.importCount.one": "Importer {count} transaction",
  "import.importCount.other": "Importer {count} transactions",
  "import.mirroredSummary": "{imported} importées, {mirrored} reconnues comme virements et reflétées sur l'autre compte.",

  // ---- Transactions page ----
  "transactions.title": "Transactions",
  "transactions.count.one": "{count} transaction",
  "transactions.count.other": "{count} transactions",
  "transactions.none": "Aucune transaction.",
  "transactions.addTransaction": "Ajouter une transaction",
  "transactions.importCsv": "Importer un CSV",
  "transactions.amountPlaceholder": "Montant (− dépense / + revenu)",
  "transactions.categoryPlaceholder": "Catégorie…",
  "transactions.accountPlaceholder": "Compte…",
  "transactions.save": "Enregistrer la transaction",
  "transactions.amountInvalid": "Le montant doit être un nombre non nul (négatif pour une dépense, positif pour un revenu).",
  "transactions.chooseCategoryAndAccount": "Choisissez une catégorie et un compte.",
  "transactions.deleted": "Transaction supprimée.",
  "transactions.transferDeleted": "Virement supprimé, sur les deux comptes.",
  "transactions.bulkDeleted.one": "{count} transaction supprimée.",
  "transactions.bulkDeleted.other": "{count} transactions supprimées.",
  "transactions.bulkDeletedWithTransfers.one": "{count} transactions supprimées ({groups} virement retiré des deux comptes).",
  "transactions.bulkDeletedWithTransfers.other": "{count} transactions supprimées ({groups} virements retirés des deux comptes).",
  "transactions.unanchoredWarning": "Les soldes de {names} sont faux tant que vous n'avez pas défini de point de départ — faites-le dans Comptes.",
  "transactions.filterByAmount": "Filtrer par montant",
  "transactions.min": "Min",
  "transactions.max": "Max",
  "transactions.type": "Type",
  "transactions.allCategories": "Toutes les catégories",
  "transactions.allAccounts": "Tous les comptes",
  "transactions.allTypes": "Tous les types",
  "transactions.filterByDescription": "Filtrer par libellé",
  "transactions.filterByType": "Filtrer par type",
  "transactions.filterByCategory": "Filtrer par catégorie",
  "transactions.filterByAccount": "Filtrer par compte",
  "transactions.searchDescription": "Rechercher un libellé…",
  "transactions.searchType": "Rechercher un type…",
  "transactions.transferBadge": "virement",
  "transactions.transferBadgeTitle": "Entre vos propres comptes — non compté comme une dépense",
  "transactions.adjustmentBadge": "ajustement",
  "transactions.adjustmentBadgeTitle": "Ajustement de solde, saisi depuis Comptes — non compté comme une dépense",
  "transactions.deleteTransaction": "Supprimer la transaction",
  "transactions.selectTransaction": "Sélectionner la transaction {date} {description}",
  "transactions.showExpenses": "Afficher les dépenses",
  "transactions.hideExpenses": "Masquer les dépenses",
  "transactions.showIncome": "Afficher les revenus",
  "transactions.hideIncome": "Masquer les revenus",
  "transactions.resetExpenses": "Réinitialiser le tri et les filtres des dépenses",
  "transactions.resetIncome": "Réinitialiser le tri et les filtres des revenus",
  "transactions.resetTitle": "Réinitialiser le tri et les filtres",
  "transactions.reset": "Réinitialiser",
  "transactions.bulkExpenses": "Actions groupées sur les dépenses",
  "transactions.bulkIncome": "Actions groupées sur les revenus",
  "transactions.recategorizeExpenses": "Recatégoriser les dépenses sélectionnées",
  "transactions.recategorizeIncome": "Recatégoriser les revenus sélectionnés",
  "transactions.selectedCount": "{count} sélectionnées",
  "transactions.deleteSelected": "Supprimer {count} transactions",
  "transactions.loadingMore": "Chargement…",
  "transactions.allLoaded": "Toutes les transactions sont chargées.",
  "transactions.scrollToTop": "Revenir en haut",

  // ---- Details page ----
  "details.title": "Détails",
  "details.month": "Mois",
  "details.year": "Année",
  "details.allTime": "Tout",
  "details.setDates": "Choisir les dates",
  "details.compare": "Comparer",
  "details.compareTitle": "Comparer cette période à une autre",
  "details.compareDisabled": "« Tout » est une période unique — il n'y a rien à quoi la comparer",
  "details.previousPeriod": "{period} précédent",
  "details.previousPeriodKey": "{period} précédent (←)",
  "details.nextPeriod": "{period} suivant",
  "details.nextPeriodKey": "{period} suivant (→)",
  "details.previousComparison": "{period} de comparaison précédent",
  "details.nextComparison": "{period} de comparaison suivant",
  "details.period.month": "mois",
  "details.period.year": "année",
  "details.daysSoFar": "{elapsed} jours sur {total} à ce jour",
  "details.precedingSpan": "Période précédente",
  "details.lengthMismatch": "{daysA} jours contre {daysB} — durées différentes",
  "details.viewTransactions": "Voir les transactions de {name}",
  "details.show": "Afficher {name}",
  "details.hide": "Masquer {name}",
  "details.noChange": "sans changement",
  "details.shareOfPanel": "{period} — part des {panel}",
  "details.shareOfSlice": "Part de {name}",
  "details.shareOfTotal": "Part du total des {panel}",
  "details.leftThisPeriod": "Reste sur la période",
  "details.was": "était {amount}",
  "details.latestMonth": "Ce mois-ci est le plus récent",
  "details.latestYear": "Cette année est la plus récente",
  "details.vs": "contre",

  // ---- Overview page ----
  "overview.title": "Vue d'ensemble",
  "overview.noAccountsBefore": "Aucun compte pour l'instant. Rendez-vous dans",
  "overview.noAccountsAfter": "pour en ajouter un.",
  "overview.totalAvailable": "Total disponible",
  "overview.noMovement": "Aucun mouvement ce mois-ci",
  "overview.movementThisMonth": "{amount} ce mois-ci",
  "overview.thisMonth": "Ce mois-ci",
  "overview.spentSoFar": "Dépensé à ce jour",
  "overview.sinceTheFirst": "Depuis le 1er",
  "overview.vsLastMonth": "vs. même date le mois dernier",
  "overview.nothingSpentEither": "Rien de dépensé ni ce mois-ci ni le mois dernier",
  "overview.more": "de plus",
  "overview.less": "de moins",
  "overview.identical": "identique",
  "overview.lastMonthByDay": "{amount} au {day} du mois dernier",
  "overview.meanMonthlySpend": "Dépense mensuelle médiane",
  "overview.meanSpendOptions": "Options de dépense mensuelle médiane",
  "overview.editRentCategory": "Modifier la catégorie loyer",
  "overview.withRent": "Avec le loyer",
  "overview.withoutRent": "Hors loyer",
  "overview.overLastMonths": "Sur les {months} derniers mois",
  "overview.meanMonthlySavings": "Épargne mensuelle moyenne",
  "overview.incomeExpensesByMonth": "Revenus & dépenses par mois",
  "overview.savings": "Épargne",
  "overview.noTransactionsInMonths": "Aucune transaction sur les {months} derniers mois.",
  "overview.balanceOverTime": "Évolution du solde",
  "overview.overMonths": "sur {months} mois",
  "overview.notEnoughHistory": "Pas encore assez d'historique — l'évolution du solde demande au moins deux mois.",
  "overview.monthIncome": "Revenus de {month} : {amount}",
  "overview.monthExpenses": "Dépenses de {month} : {amount}",
  "overview.monthSavings": "Épargne de {month} : {amount}",
  "overview.endOfMonth": "Fin {month} : {amount}",
  "overview.recurring": "Engagements récurrents",
  "overview.perMonthAcross.one": "{amount} / mois sur {count} prélèvement",
  "overview.perMonthAcross.other": "{amount} / mois sur {count} prélèvements",
  "overview.noRecurring": "Rien de détecté pour l'instant. Un prélèvement doit apparaître au moins trois fois, à un rythme régulier et pour un montant à peu près constant, avant d'être considéré comme récurrent.",
  "overview.nextOn": "Prochain le {date}",
  "overview.perMonthNext": "≈ {amount}/mois · prochain le {date}",
  "overview.showFewer": "Afficher moins",
  "overview.showAllRecurring": "Afficher les {count} prélèvements récurrents",
  "overview.notSeenRecently": "Non vus récemment",
  "overview.lapsedHint": "Ces prélèvements suivaient un rythme puis se sont arrêtés. Soit ils ont été résiliés, soit un paiement a échoué et mérite vérification.",
  "overview.lastSeen": "Vu pour la dernière fois le {date}",
  "operationKind.card": "Carte",
  "operationKind.bank_transfer": "Virement",
  "operationKind.direct_debit": "Prélèvement",
  "operationKind.check": "Chèque",
  "operationKind.cash": "Espèces",
  "operationKind.fees": "Frais",
  "operationKind.other": "Autre",
  "cadence.weekly": "hebdomadaire",
  "cadence.monthly": "mensuel",
  "cadence.quarterly": "trimestriel",
  "cadence.yearly": "annuel",

  // ---- Accounts page ----
  "accounts.title": "Comptes",
  "accounts.dragToReorder": "Glisser pour réorganiser",
  "accounts.namePlaceholder": "Nom du compte",
  "accounts.addAccount": "Ajouter un compte",
  "accounts.empty": "Aucun compte pour l'instant — ajoutez-en un ci-dessus.",
  "accounts.balanceLabel": "solde : {amount}",
  "accounts.default": "par défaut",
  "accounts.setAsDefault": "Définir par défaut",
  "accounts.editStartingPoint": "Modifier le point de départ",
  "accounts.setStartingPoint": "Définir le point de départ",
  "accounts.addAdjustment": "Ajouter un ajustement",
  "accounts.deleteAccount": "Supprimer le compte",
  "accounts.deleted": "« {name} » supprimé.",
  "accounts.balanceNotANumber": "Le solde doit être un nombre.",
  "accounts.startingPointUpdated": "Point de départ mis à jour pour « {name} ».",
  "accounts.startingPointSet": "Point de départ défini pour « {name} ».",
  "accounts.unanchored": "Point de départ non défini — ce solde ne reflète que les transactions enregistrées, il est donc décalé de ce que le compte contenait avant elles.",
  "accounts.noLedgerEntry": "— aucune écriture n'est ajoutée au registre",
  "accounts.bankBalanceToday": "Solde affiché aujourd'hui par votre banque",
  "accounts.transactionsOnRecord": "Transactions enregistrées",
  "accounts.startingPointNow": "Point de départ actuel",
  "accounts.startingPointBecomes": "Nouveau point de départ",
  "accounts.anchorHint": "À utiliser quand le solde est faux depuis le début. Calcule ce que le compte contenait avant votre première transaction enregistrée, ce qui corrige tous les soldes passés d'un coup.",
  "accounts.anchorHintReplaces": "Ceci remplace entièrement le point de départ — mais n'annule pas un ajustement saisi par erreur, il l'absorbe seulement. Supprimez d'abord cette écriture dans Transactions s'il y en a une.",
  "accounts.oneEntryDatedToday": "— une écriture, datée d'aujourd'hui",
  "accounts.appCurrentlyShows": "L'application affiche actuellement",
  "accounts.adjustmentPosted": "Ajustement enregistré",
  "accounts.adjustmentNone": "aucun — déjà concordant",
  "accounts.adjustedBy": "Ajusté de {amount}.",
  "accounts.alreadyMatched": "« {name} » concordait déjà — rien à ajuster.",
  "accounts.reconcileHint": "À utiliser quand de l'argent a bougé sans que vous l'importiez — frais, intérêts, variations de marché. Les soldes passés restent inchangés et l'ajustement ne compte pas comme une dépense.",
  "accounts.belongsToThisAccount": "Appartient à ce compte",
  "accounts.belongsTitle": "Comparé au libellé d'une ligne importée pour déterminer à quel compte elle appartient",
  "accounts.removePattern": "Retirer le motif",
  "accounts.addPatternPlaceholder": "Ajouter un motif de libellé…",
  "accounts.transfersInto": "Virements vers ce compte",
  "accounts.transfersIntoTitle": "Une ligne importée correspondant à l'un de ces motifs est de l'argent que vous avez envoyé vers ce compte — elle est reflétée ici automatiquement et exclue des totaux de dépenses",
  "accounts.removeTransferRule": "Retirer la règle de virement",
  "accounts.addTransferPatternPlaceholder": "Ajouter un motif de virement…",
  "accounts.applyToPast": "Appliquer aux transactions passées",
  "accounts.applyRulesHint": "Réanalyse toutes les transactions déjà enregistrées avec les motifs de virement ci-dessus, en convertissant chaque correspondance en paire de virement — exactement ce qu'un nouvel import aurait fait, pour les lignes importées avant l'existence du motif.",
  "accounts.converted.one": "{count} transaction existante convertie en virement.",
  "accounts.converted.other": "{count} transactions existantes converties en virements.",
  "accounts.noMatchesFound": "Aucune transaction correspondante trouvée.",

  // ---- Categories page ----
  "categories.title": "Catégories",
  "categories.summary": "{categories} catégories · {subcategories} sous-catégories",
  "categories.newNamePlaceholder": "Nom de la nouvelle catégorie",
  "categories.addCategory": "Ajouter une catégorie",
  "categories.empty": "Aucune catégorie pour l'instant — ajoutez-en une ci-dessus.",
  "categories.deleted": "« {name} » supprimée.",
  "categories.reassignDialogLabel": "Réaffecter les transactions avant suppression",
  "categories.reassignPrompt": "« {name} » contient encore des transactions. Choisissez une catégorie où les déplacer avant de supprimer :",
  "categories.selectCategory": "Choisissez une catégorie…",
  "categories.searchCategory": "Rechercher une catégorie…",
  "categories.reassignAndDelete": "Réaffecter et supprimer",

  // ---- Shared components ----
  "component.deleteConfirm": "Supprimer ?",
  "component.confirmDelete": "Confirmer la suppression",
  "component.previous": "Précédent",
  "component.next": "Suivant",
  "component.previousMonth": "Mois précédent",
  "component.nextMonth": "Mois suivant",
  "component.byDate": "Par date",
  "component.byMonth": "Par mois",
  "component.dismiss": "Fermer",
  "component.select": "Sélectionner…",
  "component.searchPlaceholder": "Rechercher…",
  "component.noMatches": "Aucun résultat.",
  "find.placeholder": "Rechercher dans la page…",
  "find.unsupported": "non pris en charge",
  "find.noResults": "Aucun résultat",
  "find.previousMatch": "Résultat précédent",
  "find.nextMatch": "Résultat suivant",
  "find.close": "Fermer la barre de recherche",
  "categoryCard.changeIcon": "Changer l'icône",
  "categoryCard.categoryName": "Nom de la catégorie",
  "categoryCard.subcategoryName": "Nom de la sous-catégorie",
  "categoryCard.deleteCategory": "Supprimer la catégorie",
  "categoryCard.deleteSubcategory": "Supprimer la sous-catégorie",
  "categoryCard.addSubcategory": "Ajouter une sous-catégorie",
  "categoryCard.subcategoryPlaceholder": "Sous-catégorie",

  // ---- Command palette ----
  "palette.placeholder": "Tapez une commande ou recherchez…",
  "palette.noMatches": "Aucune commande correspondante.",
  "palette.navigate": "Naviguer",
  "palette.actions": "Actions",
  "palette.nextPage": "Page suivante",
  "palette.previousPage": "Page précédente",
  "palette.goTo": "Aller à {page}",
  "palette.addTransaction": "Ajouter une transaction",
  "palette.importCsv": "Importer un CSV",

  // ---- Unlock / create ----
  "unlock.tagline": "Choisissez une phrase secrète pour chiffrer vos données locales. Il n'y a aucune récupération possible — si vous la perdez, vos données sont illisibles.",
  "unlock.enterPassphrase": "Saisissez votre phrase secrète pour déverrouiller vos données.",
  "unlock.passphrase": "Phrase secrète",
  "unlock.confirmPassphrase": "Confirmez la phrase secrète",
  "unlock.create": "Créer la base chiffrée",
  "unlock.unlock": "Déverrouiller",
  "unlock.tooShort": "La phrase secrète doit contenir au moins {min} caractères.",
  "unlock.mismatch": "Les phrases secrètes ne correspondent pas.",
  "unlock.empty": "La phrase secrète ne peut pas être vide.",

  // ---- Date ranges ----
  "range.thisMonth": "Ce mois-ci",
  "range.thisYear": "Cette année",
  "range.allTime": "Depuis le début",
  "range.custom": "Personnalisé",
  "range.setDates": "Choisir les dates",
  "range.from": "Du",
  "range.to": "Au",
  "range.apply": "Appliquer",

  // ---- Settings ----
  "settings.title": "Réglages",
  "settings.language": "Langue",
  "settings.languageHelp": "Change la langue de l'interface. Les catégories par défaut que vous n'avez pas renommées sont également traduites ; celles que vous avez renommées ne sont pas touchées.",
  "settings.languageSaved": "Langue définie sur {language}.",
  "settings.categoriesRelabelled.one": "{count} catégorie par défaut a été traduite.",
  "settings.categoriesRelabelled.other": "{count} catégories par défaut ont été traduites.",
  "settings.currency": "Devise",
  "settings.currencySaved": "Devise définie sur {code}.",
  "settings.autoLock": "Verrouillage automatique",
  "settings.autoLockSaved": "Verrouillage automatique réglé sur {label}.",
  "settings.autoLock.never": "Jamais",
  "settings.autoLock.oneMinute": "1 minute",
  "settings.autoLock.tenMinutes": "10 minutes",
  "settings.autoLock.oneHour": "1 heure",
  "settings.currencyHint": "Change uniquement l'affichage — les transactions passées ne sont pas converties.",
  "settings.autoLockHint": "Verrouille l'application et redemande votre phrase secrète après ce délai sans activité clavier ou souris.",
  "settings.passphrase": "Phrase secrète",
  "settings.passphraseHint": "Changez la phrase secrète qui chiffre votre base de données.",
  "settings.changePassphrase": "Changer la phrase secrète",
  "settings.passphraseNoRecovery": "Aucune récupération possible — si vous perdez la nouvelle phrase secrète, vos données sont illisibles.",
  "settings.currentPassphrase": "Phrase secrète actuelle",
  "settings.newPassphrase": "Nouvelle phrase secrète",
  "settings.confirmNewPassphrase": "Confirmez la nouvelle phrase secrète",
  "settings.passphraseMinimum": "Au moins {min} caractères.",
  "settings.passphraseTooShort": "La nouvelle phrase secrète doit contenir au moins {min} caractères.",
  "settings.passphraseMismatch": "Les nouvelles phrases secrètes ne correspondent pas.",
  "settings.passphraseChanged": "Phrase secrète modifiée.",
  "settings.changing": "Modification…",
  "settings.exportDatabase": "Exporter la base",
  "settings.exportDatabaseHint": "Enregistre une copie chiffrée de votre fichier de base de données.",
  "settings.export": "Exporter",
  "settings.exporting": "Export…",
  "settings.exportedTo": "Exporté vers {path}",
  "settings.exportCsv": "Exporter les transactions en CSV",
  "settings.exportCsvHint": "Enregistre les transactions d'un compte dans un fichier CSV, lisible en dehors de Scrat.",
  "settings.exportCsvButton": "Exporter en CSV",
  "settings.noAccountsToExport": "Ajoutez d'abord un compte — il n'y a rien à exporter pour l'instant.",
  "settings.chooseAccount": "Choisissez un compte…",
  "settings.searchAccount": "Rechercher un compte…",
  "settings.importDatabase": "Importer une base",
  "settings.importDatabaseHint": "Remplace tout le contenu de Scrat par un autre fichier de base chiffré.",
  "settings.chooseFileToImport": "Choisir le fichier à importer",
  "settings.importWarningStrong": "Ceci remplacera définitivement votre base actuelle",
  "settings.importWarningRest": "par {file}. C'est irréversible — exportez d'abord votre base actuelle si vous voulez en garder une copie.",
  "settings.importPassphrase": "Phrase secrète du fichier importé",
  "settings.replaceDatabase": "Remplacer la base",
  "settings.importing": "Import…",
  "settings.databaseImported": "Base importée.",
  "settings.contactTitle": "Signaler un bug ou contacter le mainteneur",
  "settings.contactHint": "Un bug, ou une question ? Écrivez à",
  "settings.sendEmail": "Envoyer un e-mail",
  "settings.mailAppFailed": "Impossible d'ouvrir votre messagerie — écrivez à {address}.",
  "settings.deleteTitle": "Supprimer mes données",
  "settings.deleteHint": "Supprime définitivement votre base locale. Aucune sauvegarde n'est faite.",
  "settings.deleteWarningStrong": "Ceci supprimera définitivement toutes vos données.",
  "settings.deleteWarningRest": "Il n'y a ni annulation ni sauvegarde. Tapez {word} ci-dessous pour confirmer.",
  "settings.deleteConfirmLabel": "Tapez {word} pour confirmer",
  "settings.deletePermanently": "Supprimer définitivement",
  "settings.deleting": "Suppression…",
  "settings.dataDeleted": "Vos données ont été supprimées.",

  // ---- Errors from the backend ----
  "error.db_locked": "La base est verrouillée. Déverrouillez-la et réessayez.",
  "error.db_already_exists": "Une base existe déjà sur cette machine.",
  "error.incorrect_passphrase": "Phrase secrète incorrecte.",
  "error.passphrase_empty": "La phrase secrète ne peut pas être vide.",
  "error.passphrase_too_short": "La phrase secrète doit contenir au moins {min} caractères.",
  "error.app_data_dir_unavailable": "Impossible de déterminer où stocker vos données : {detail}",
  "error.database_error": "Erreur de base de données : {detail}",
  "error.filesystem_error": "Erreur de fichier : {detail}",
  "error.account_not_found": "Ce compte n'existe plus.",
  "error.category_not_found": "Cette catégorie n'existe plus.",
  "error.account_name_empty": "Un compte doit avoir un nom.",
  "error.account_name_too_long": "Un nom de compte ne peut pas dépasser {max} caractères.",
  "error.account_pattern_empty": "Le motif ne peut pas être vide.",
  "error.category_name_empty": "Une catégorie doit avoir un nom.",
  "error.category_name_too_long": "Un nom de catégorie ne peut pas dépasser {max} caractères.",
  "error.category_self_parent": "Une catégorie ne peut pas être sa propre catégorie parente.",
  "error.category_unknown_icon": "« {icon} » n'est pas une icône connue de cette version.",
  "error.subcategory_cannot_have_icon": "Une sous-catégorie ne peut pas avoir sa propre icône.",
  "error.category_seed_key_empty": "L'identifiant interne de cette catégorie est invalide.",
  "error.amount_zero": "Le montant ne peut pas être nul.",
  "error.description_empty": "Un libellé est obligatoire.",
  "error.description_too_long": "Un libellé ne peut pas dépasser {max} caractères.",
  "error.transfer_without_group": "Un virement doit avoir une contrepartie.",
  "error.group_without_transfer_role": "Seul un virement peut appartenir à une paire de virement.",
  "error.unknown_transaction_role": "« {value} » n'est pas un type de transaction connu de cette version.",
  "error.unknown_operation_kind": "« {value} » n'est pas un moyen de paiement connu de cette version.",
  "error.invalid_currency_code": "« {value} » n'est pas un code de devise valide.",
  "error.currency_mismatch": "Impossible de combiner des montants en {left} et en {right}.",
  "error.invalid_id": "« {value} » n'est pas un identifiant valide.",
  "error.invalid_date": "« {value} » n'est pas une date valide.",
  "error.account_has_transactions": "Ce compte contient encore {count} transaction(s). Réaffectez-les ou supprimez-les d'abord.",
  "error.category_requires_reassignment": "Cette catégorie contient encore {count} transaction(s). Choisissez une catégorie où les déplacer.",
  "error.default_category_protected": "La catégorie par défaut ne peut être ni renommée ni supprimée.",
  "error.parent_is_subcategory": "Une sous-catégorie ne peut pas elle-même contenir des sous-catégories.",
  "error.category_has_subcategories": "Cette catégorie a ses propres sous-catégories, elle ne peut donc pas en devenir une.",
  "error.duplicate_transfer_rule": "Une règle de virement pour {pattern} existe déjà.",
  "error.balance_out_of_range": "Ce solde est trop grand pour être traité.",
  "error.invalid_reorder": "Impossible de réorganiser les comptes — essayez de recharger la page.",
  "error.unsupported_language": "« {value} » n'est pas une langue prise en charge par cette version.",
  "error.auto_lock_invalid": "Le verrouillage automatique doit être jamais, 1, 10 ou 60 minutes.",
  "error.auto_lock_stored_invalid": "Le réglage de verrouillage automatique enregistré est invalide.",
  "error.nothing_to_export": "Il n'y a pas encore de base à exporter.",
  "error.import_file_missing": "Le fichier sélectionné n'existe pas.",
  "error.import_finalize_failed": "Impossible de terminer la restauration ({detail}). Votre base d'origine n'a pas été modifiée — relancez l'application et déverrouillez-la avec votre phrase secrète d'origine.",
  "error.import_reopen_failed": "La base a été remplacée mais n'a pas pu être rouverte ({detail}). Relancez l'application et déverrouillez-la avec la phrase secrète du fichier restauré.",
  "error.csv_file_too_large": "Ce fichier fait {size_mb} Mo — trop volumineux pour un export CSV (limite : {limit_mb} Mo).",
  "error.no_destination_account": "Aucun compte de destination choisi et aucun compte par défaut défini — choisissez-en un, ou définissez un compte par défaut dans Comptes.",
  "error.too_many_selected": "Trop de transactions sélectionnées ({max} au maximum).",
  "error.unknown": "Une erreur s'est produite ({code}).",
};

const MESSAGES: Record<Language, Record<MessageKey, string>> = { en, fr };

/* ------------------------------------------------------------------ *
 * The reactive language holder
 * ------------------------------------------------------------------ */

/** The one instance for the app's lifetime — not per-page state, exactly like
 * `session`. */
class I18n {
  /** Seeded from the cache so the passphrase screen renders in the right
   * language before any database is open; replaced by the database's value
   * as soon as one is unlocked. */
  language = $state<Language>(DEFAULT_LANGUAGE);

  /** Called once the database is open (and again after a language change), so
   * the whole UI switches without a reload. */
  setLanguage(language: Language) {
    this.language = language;
    rememberLanguage(language);
  }

  /** Restores the cached choice for the pre-unlock screen. Separate from
   * `setLanguage` because it must not write back — there is nothing new to
   * remember, and a database may be about to disagree. */
  restoreCached() {
    this.language = cachedLanguage();
  }
}

export const i18n = new I18n();

/* ------------------------------------------------------------------ *
 * Lookup
 * ------------------------------------------------------------------ */

function interpolate(template: string, params?: Record<string, string | number>): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}

/** Translates `key` in the current language.
 *
 * Reads `i18n.language`, so calling this in markup makes that markup react to
 * a language change on its own. */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  // Falling back to the key rather than throwing: a missing message is a bug,
  // but it is a cosmetic one, and a `t()` that throws takes the whole
  // component render down with it. `Record<MessageKey, string>` on `fr` makes
  // the *translated* case unreachable; what this actually catches is a key
  // built at runtime (`cadence.${x}`) from a value the dictionary has never
  // heard of, where showing the raw value is the honest answer.
  const message = MESSAGES[i18n.language][key] ?? en[key] ?? key;
  return interpolate(message, params);
}

/** Plural form of `key`, which must exist as `key.one` and `key.other`.
 *
 * The two languages genuinely differ: English treats only 1 as singular,
 * French also treats 0 (*0 catégorie*, not *0 catégories*). Hardcoding
 * English's rule would produce a subtly wrong sentence in French on the one
 * count — zero — that a "nothing happened" message hits most often.
 *
 * `count` is passed through as a parameter, so `{count}` works in the message
 * without the caller repeating it. */
export function tp(
  key: string,
  count: number,
  params?: Record<string, string | number>,
): string {
  const plural = i18n.language === "fr" ? Math.abs(count) >= 2 : count !== 1;
  const form = `${key}.${plural ? "other" : "one"}` as MessageKey;
  return t(form, { count, ...params });
}

/* ------------------------------------------------------------------ *
 * Locale-dependent formatting
 * ------------------------------------------------------------------ */

/* Written out rather than taken from `Intl`. Two reasons, and the second is
   the load-bearing one:

   1. The app already hand-rolls its date and money formatting (see `api.ts`),
      because it renders fixed layouts — a chart axis gutter, a fixed-width
      calendar header — where a name's length is part of the design.
   2. `Intl` reads the *host* locale data, which is the OS's, not the app's.
      A French user on an English macOS would get English month names inside a
      French interface, and nothing in this codebase would explain why. The
      language setting has to be the only thing that decides. */

const MONTH_NAMES_BY_LANGUAGE: Record<Language, string[]> = {
  en: [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
  ],
  fr: [
    "janvier", "février", "mars", "avril", "mai", "juin",
    "juillet", "août", "septembre", "octobre", "novembre", "décembre",
  ],
};

const SHORT_MONTH_NAMES_BY_LANGUAGE: Record<Language, string[]> = {
  en: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
  // Not the English names truncated: French abbreviates *juillet* as "juil."
  // and keeps its accents, and four-letter forms are the convention there.
  fr: ["janv", "févr", "mars", "avr", "mai", "juin", "juil", "août", "sept", "oct", "nov", "déc"],
};

/** Monday-first in both languages — the app's calendars start the week on
 * Monday, which is the convention in France and the UK alike. */
const WEEKDAY_LABELS_BY_LANGUAGE: Record<Language, string[]> = {
  en: ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"],
  fr: ["Lu", "Ma", "Me", "Je", "Ve", "Sa", "Di"],
};

export function monthNames(): string[] {
  return MONTH_NAMES_BY_LANGUAGE[i18n.language];
}

export function shortMonthNames(): string[] {
  return SHORT_MONTH_NAMES_BY_LANGUAGE[i18n.language];
}

export function weekdayLabels(): string[] {
  return WEEKDAY_LABELS_BY_LANGUAGE[i18n.language];
}

/** How the current language writes numbers. English groups with commas and
 * separates decimals with a dot; French groups with a narrow space and uses a
 * comma. Getting this wrong doesn't just look foreign — `1,234` reads as one
 * thousand to an English speaker and as one-and-a-bit to a French one. */
export interface NumberSeparators {
  group: string;
  decimal: string;
}

export function numberSeparators(): NumberSeparators {
  return i18n.language === "fr"
    ? { group: " ", decimal: "," } // narrow no-break space, per French typography
    : { group: ",", decimal: "." };
}

/** Whether the language writes a space before the unit in "12 %" (French
 * does; English does not). */
export function spaceBeforeUnit(): string {
  return i18n.language === "fr" ? " " : "";
}

/* ------------------------------------------------------------------ *
 * Backend errors
 * ------------------------------------------------------------------ */

/** The error shape `AppError` serializes to — see `src-tauri/src/errors.rs`. */
export interface BackendError {
  code: string;
  params?: Record<string, string>;
}

function isBackendError(value: unknown): value is BackendError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    typeof (value as { code: unknown }).code === "string"
  );
}

/** Renders anything thrown by an `api.*` call as a sentence in the user's
 * language.
 *
 * Three cases, in order of how much is known:
 *  - a code with a message → the translated sentence;
 *  - a code without one → the generic sentence, *naming the code*, because a
 *    user reporting "something went wrong (foo_bar)" can be helped and a user
 *    reporting "something went wrong" cannot;
 *  - not a backend error at all (a thrown `TypeError`, a bug in the frontend,
 *    the missing Tauri bridge in a browser tab) → its own text, untranslated.
 *    Inventing a friendly sentence for those would hide the only diagnostic
 *    there is. */
/** The backend's code for a thrown error, or `null` if it isn't one.
 *
 * For the handful of places that need to *branch* on which failure happened,
 * not just show it. Before the codes existed those places matched on the
 * English prose (`message.includes("reassign")`), which is a sniff that a
 * translated build would have silently stopped matching — turning a
 * recoverable "pick somewhere to move these transactions" prompt into a bare
 * error toast for every French user. */
export function errorCode(error: unknown): string | null {
  return isBackendError(error) ? error.code : null;
}

export function describeError(error: unknown): string {
  if (!isBackendError(error)) return String(error);
  const key = `error.${error.code}` as MessageKey;
  if (key in en) return t(key, error.params);
  return t("error.unknown", { code: error.code });
}
