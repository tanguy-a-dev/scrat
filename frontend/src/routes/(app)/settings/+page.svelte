<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { save, open } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { getVersion } from "@tauri-apps/api/app";
  import SearchSelect from "$lib/SearchSelect.svelte";
  import { api, type AccountDto } from "$lib/api";
  import { clearPageCache } from "$lib/pageCache";
  import { toast } from "$lib/toasts.svelte";
  import { session } from "$lib/session.svelte";
  import {
    LANGUAGES,
    LANGUAGE_LABELS,
    describeError,
    i18n,
    isLanguage,
    t,
    tp,
    type Language,
  } from "$lib/i18n.svelte";

  const COMMON_CURRENCIES = ["USD", "EUR", "GBP", "CAD", "AUD", "CHF", "JPY"];

  /** Moves focus to the field a panel was opened for. Each of the three
   * confirm-then-act flows below replaces its trigger button with a form, so
   * without this the focused element is destroyed on open and focus falls
   * back to the document — a keyboard user would have to tab in from the top
   * of the page to reach a panel they just asked for. Same one-liner as the
   * transactions page's. */
  function autofocus(node: HTMLElement) {
    node.focus();
  }

  let currentCurrency = $state("");
  let selectedCurrency = $state("");
  let loadError = $state("");
  let loadingCurrency = $state(true);

  /* `$derived` so the labels follow a language change, rather than being
     frozen at whatever the language was when this page first mounted. */
  const AUTO_LOCK_OPTIONS = $derived([
    { minutes: 1, label: t("settings.autoLock.oneMinute") },
    { minutes: 10, label: t("settings.autoLock.tenMinutes") },
    { minutes: 60, label: t("settings.autoLock.oneHour") },
    { minutes: 0, label: t("settings.autoLock.never") },
  ]);

  /* The language section. `selectedLanguage` tracks the picker; `i18n.language`
     is what the app is actually rendering in, and only the Save button moves
     it — a select that retranslated the page on every keyboard arrow press
     would relabel categories in the database as the user scrolled past. */
  let selectedLanguage = $state<Language>(i18n.language);
  let savingLanguage = $state(false);

  let currentAutoLockMinutes = $state(10);
  let selectedAutoLockMinutes = $state(10);
  let autoLockLoadError = $state("");
  let loadingAutoLock = $state(true);

  /** The offered codes, plus whatever is actually stored if it isn't one of
   * them. The backend accepts any ISO-shaped code (`Currency::new` takes
   * three uppercase letters), so a database can legitimately arrive — via
   * import, or from a build whose list was longer — carrying a code this
   * list never had. Without it as an option, `bind:value` matches nothing:
   * the select renders blank as though no currency were set, and the first
   * touch of it silently overwrites a perfectly valid setting. */
  let currencyOptions = $derived(
    currentCurrency && !COMMON_CURRENCIES.includes(currentCurrency)
      ? [...COMMON_CURRENCIES, currentCurrency]
      : COMMON_CURRENCIES,
  );

  let exporting = $state(false);
  let exportingCsv = $state(false);

  /** The CSV export is per-account, so the picker needs the account list.
   * Failing to load it leaves the export disabled with a reason shown,
   * rather than an empty picker that looks like "you have no accounts". */
  let accounts = $state<AccountDto[]>([]);
  let accountsLoadError = $state("");
  let csvAccountId = $state("");

  let csvAccountOptions = $derived(
    accounts.map((a) => ({ id: a.id, label: a.name })),
  );
  let csvAccountName = $derived(
    accounts.find((a) => a.id === csvAccountId)?.name ?? "",
  );

  let importPath = $state<string | null>(null);
  let importPassword = $state("");
  let importing = $state(false);
  let importFileName = $derived(importPath?.split(/[\\/]/).pop() ?? "");

  onMount(async () => {
    // `session.markUnlocked` already loaded the language from this database;
    // mirror it into the picker rather than reading it again.
    selectedLanguage = i18n.language;

    try {
      currentCurrency = await api.getCurrency();
      selectedCurrency = currentCurrency;
    } catch (e) {
      loadError = describeError(e);
    } finally {
      loadingCurrency = false;
    }

    try {
      currentAutoLockMinutes = await api.getAutoLockMinutes();
      selectedAutoLockMinutes = currentAutoLockMinutes;
    } catch (e) {
      autoLockLoadError = describeError(e);
    } finally {
      loadingAutoLock = false;
    }

    try {
      accounts = await api.listAccounts();
      // Only preselect when there's no choice to make. With several accounts
      // the file's scope is the whole point of the feature, so it should be
      // something the user picked, not something they inherited from
      // whichever account happened to sort first.
      if (accounts.length === 1) csvAccountId = accounts[0].id;
    } catch (e) {
      accountsLoadError = describeError(e);
    }

    try {
      appVersion = await getVersion();
    } catch {
      // Version is a nicety in the contact section, not a reason to show an
      // error — leave it blank.
    }
  });

  async function handleSaveCurrency(event: Event) {
    event.preventDefault();
    try {
      await api.setCurrency(selectedCurrency);
      currentCurrency = selectedCurrency;
      toast.success(t("settings.currencySaved", { code: selectedCurrency }));
    } catch (e) {
      toast.error(describeError(e));
    }
  }

  /** Saves the language, then switches the running UI to it.
   *
   * The order matters: `api.setLanguage` is what relabels the seeded
   * categories in the database, and only once it has returned is the new
   * language true of anything. Flipping `i18n` first would leave a failed
   * save showing a French interface over an English database. */
  async function handleSaveLanguage(event: Event) {
    event.preventDefault();
    savingLanguage = true;
    try {
      const relabelled = await api.setLanguage(selectedLanguage);
      i18n.setLanguage(selectedLanguage);
      // Category names are cached per page; they have just changed underneath
      // every one of them.
      clearPageCache();
      toast.success(
        relabelled > 0
          ? `${t("settings.languageSaved", { language: LANGUAGE_LABELS[selectedLanguage] })} ${tp("settings.categoriesRelabelled", relabelled)}`
          : t("settings.languageSaved", { language: LANGUAGE_LABELS[selectedLanguage] }),
      );
    } catch (e) {
      // Put the picker back where the app actually is, so it never claims a
      // language the database didn't accept.
      selectedLanguage = i18n.language;
      toast.error(describeError(e));
    } finally {
      savingLanguage = false;
    }
  }

  async function handleSaveAutoLock(event: Event) {
    event.preventDefault();
    try {
      await api.setAutoLockMinutes(selectedAutoLockMinutes);
      currentAutoLockMinutes = selectedAutoLockMinutes;
      // Applies immediately to the running idle timer, not just future app
      // launches.
      session.autoLockMinutes = selectedAutoLockMinutes;
      const label =
        AUTO_LOCK_OPTIONS.find((o) => o.minutes === selectedAutoLockMinutes)
          ?.label ?? String(selectedAutoLockMinutes);
      toast.success(t("settings.autoLockSaved", { label: label.toLowerCase() }));
    } catch (e) {
      toast.error(describeError(e));
    }
  }

  // The busy flag goes up only once a destination is chosen. Raising it
  // around the native save dialog too would leave the button reading
  // "Exporting…" for however long the user spends browsing the filesystem,
  // and drop it again just as the copy actually starts — the progress it
  // reports would be exactly the wrong half of the operation.
  async function handleExport() {
    try {
      const destination = await save({
        defaultPath: timestampedFileName("scrat-export", "db"),
        filters: [{ name: "Scrat Database", extensions: ["db"] }],
      });
      if (!destination) return;
      exporting = true;
      await api.exportDatabase(destination);
      toast.success(t("settings.exportedTo", { path: destination }));
    } catch (e) {
      toast.error(describeError(e));
    } finally {
      exporting = false;
    }
  }

  function timestampedFileName(prefix: string, extension: string): string {
    const now = new Date();
    const pad = (n: number) => n.toString().padStart(2, "0");
    const stamp = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}_${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
    return `${prefix}-${stamp}.${extension}`;
  }

  /** Makes an account name safe to suggest as a filename. Account names are
   * free text, so one can legitimately contain a path separator or a
   * character the platform reserves — a name like "Joint / Savings" must not
   * turn into a suggested path pointing at a directory that isn't there. */
  function fileNameSlug(name: string): string {
    return (
      name
        .replace(/[\\/:*?"<>|]/g, "-")
        .replace(/\s+/g, "-")
        .replace(/-+/g, "-")
        .replace(/^-|-$/g, "")
        .slice(0, 40) || "account"
    );
  }

  async function handleExportCsv() {
    if (!csvAccountId) return;
    try {
      const destination = await save({
        defaultPath: timestampedFileName(
          `scrat-${fileNameSlug(csvAccountName)}`,
          "csv",
        ),
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!destination) return;
      exportingCsv = true;
      await api.exportTransactionsCsv(csvAccountId, destination);
      toast.success(t("settings.exportedTo", { path: destination }));
    } catch (e) {
      toast.error(describeError(e));
    } finally {
      exportingCsv = false;
    }
  }

  async function handleChooseImportFile() {
    try {
      const selected = await open({
        filters: [{ name: "Scrat Database", extensions: ["db"] }],
      });
      if (!selected || Array.isArray(selected)) return;
      importPath = selected;
      importPassword = "";
    } catch (e) {
      toast.error(describeError(e));
    }
  }

  function cancelImport() {
    importPath = null;
    importPassword = "";
  }

  async function handleConfirmImport(event: Event) {
    event.preventDefault();
    if (!importPath) return;
    importing = true;
    try {
      await api.importDatabase(importPath, importPassword);
      // Every page's remembered filters belong to the database that was just
      // replaced — see clearPageCache.
      clearPageCache();
      // The imported database has its own auto-lock setting, potentially
      // different from the one just replaced.
      await session.markUnlocked();
      toast.success(t("settings.databaseImported"));
      await goto("/overview");
    } catch (e) {
      toast.error(describeError(e));
    } finally {
      importing = false;
    }
  }

  /** Mirrors `MIN_PASSPHRASE_LENGTH` in `src-tauri/src/db.rs`, which is the
   * check that actually enforces it — this one only lets the form say so
   * without a round trip. */
  const MIN_PASSPHRASE_LENGTH = 8;

  let showPassphraseForm = $state(false);
  let currentPassphrase = $state("");
  let newPassphrase = $state("");
  let confirmNewPassphrase = $state("");
  let passphraseError = $state("");
  let changingPassphrase = $state(false);

  function startPassphraseChange() {
    showPassphraseForm = true;
  }

  function cancelPassphraseChange() {
    showPassphraseForm = false;
    currentPassphrase = "";
    newPassphrase = "";
    confirmNewPassphrase = "";
    passphraseError = "";
  }

  async function handleChangePassphrase(event: Event) {
    event.preventDefault();
    passphraseError = "";
    if (newPassphrase.length < MIN_PASSPHRASE_LENGTH) {
      passphraseError = t("settings.passphraseTooShort", { min: MIN_PASSPHRASE_LENGTH });
      return;
    }
    if (newPassphrase !== confirmNewPassphrase) {
      passphraseError = t("settings.passphraseMismatch");
      return;
    }
    changingPassphrase = true;
    try {
      await api.changePassphrase(currentPassphrase, newPassphrase);
      toast.success(t("settings.passphraseChanged"));
      cancelPassphraseChange();
    } catch (e) {
      passphraseError = describeError(e);
    } finally {
      changingPassphrase = false;
    }
  }

  const CONTACT_EMAIL = "me@kaonashi.dev";

  /** Shown next to the contact address and prefilled into the report, because
   * "which version were you running" is the first question any bug report
   * raises and the one users can least easily answer. Blank if it can't be
   * read — the section is still useful without it. */
  let appVersion = $state("");

  let contactMailto = $derived.by(() => {
    const subject = appVersion
      ? `Scrat ${appVersion} — bug report`
      : "Scrat — bug report";
    const body = [
      "What happened:",
      "",
      "What I expected:",
      "",
      "Steps to reproduce:",
      "",
      appVersion ? `Scrat version: ${appVersion}` : "",
    ].join("\n");
    return `mailto:${CONTACT_EMAIL}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`;
  });

  /** Hands the address to the OS's default mail client rather than opening it
   * in the webview. If there's no mail client to hand it to, the address is
   * already on screen to copy — say so instead of failing silently. */
  async function handleContact() {
    try {
      await openUrl(contactMailto);
    } catch {
      toast.error(t("settings.mailAppFailed", { address: CONTACT_EMAIL }));
    }
  }

  let deleteRequested = $state(false);
  let deleteConfirmText = $state("");
  let deleting = $state(false);
  const DELETE_CONFIRM_WORD = "DELETE";

  function startDelete() {
    deleteRequested = true;
    deleteConfirmText = "";
  }

  function cancelDelete() {
    deleteRequested = false;
    deleteConfirmText = "";
  }

  async function handleConfirmDelete(event: Event) {
    event.preventDefault();
    if (deleteConfirmText !== DELETE_CONFIRM_WORD) return;
    deleting = true;
    try {
      await api.deleteDatabase();
      // The deleted database's filters must not outlive it — see
      // clearPageCache.
      clearPageCache();
      session.markLocked();
      toast.success(t("settings.dataDeleted"));
      await goto("/");
    } catch (e) {
      toast.error(describeError(e));
      deleting = false;
    }
  }
</script>

<h1>{t("settings.title")}</h1>

<section>
  <h2>{t("settings.language")}</h2>
  <p class="hint">{t("settings.languageHelp")}</p>
  <form onsubmit={handleSaveLanguage}>
    <label class="visually-hidden" for="language">{t("settings.language")}</label>
    <select
      id="language"
      value={selectedLanguage}
      onchange={(e) => {
        const chosen = e.currentTarget.value;
        if (isLanguage(chosen)) selectedLanguage = chosen;
      }}
    >
      {#each LANGUAGES as code (code)}
        <option value={code}>{LANGUAGE_LABELS[code]}</option>
      {/each}
    </select>
    <button
      type="submit"
      disabled={savingLanguage || selectedLanguage === i18n.language}
    >
      {t("common.save")}
    </button>
  </form>
</section>

<section>
  <h2>{t("settings.currency")}</h2>
  <p class="hint">{t("settings.currencyHint")}</p>
  {#if loadingCurrency}
    <p>{t("common.loading")}</p>
  {:else if loadError}
    <p class="error" role="alert">{loadError}</p>
  {:else}
    <form onsubmit={handleSaveCurrency}>
      <label class="visually-hidden" for="currency">{t("settings.currency")}</label>
      <select id="currency" bind:value={selectedCurrency}>
        {#each currencyOptions as code (code)}
          <option value={code}>{code}</option>
        {/each}
      </select>
      <button type="submit" disabled={selectedCurrency === currentCurrency}>
        {t("common.save")}
      </button>
    </form>
  {/if}
</section>

<section>
  <h2>{t("settings.autoLock")}</h2>
  <p class="hint">{t("settings.autoLockHint")}</p>
  {#if loadingAutoLock}
    <p>{t("common.loading")}</p>
  {:else if autoLockLoadError}
    <p class="error" role="alert">{autoLockLoadError}</p>
  {:else}
    <form onsubmit={handleSaveAutoLock}>
      <label class="visually-hidden" for="auto-lock">{t("settings.autoLock")}</label>
      <select id="auto-lock" bind:value={selectedAutoLockMinutes}>
        {#each AUTO_LOCK_OPTIONS as option (option.minutes)}
          <option value={option.minutes}>{option.label}</option>
        {/each}
      </select>
      <button
        type="submit"
        disabled={selectedAutoLockMinutes === currentAutoLockMinutes}
      >
        {t("common.save")}
      </button>
    </form>
  {/if}
</section>

<section>
  <h2>{t("settings.passphrase")}</h2>
  {#if !showPassphraseForm}
    <p class="hint">{t("settings.passphraseHint")}</p>
    <button type="button" onclick={startPassphraseChange}>
      {t("settings.changePassphrase")}
    </button>
  {:else}
    <p class="hint">{t("settings.passphraseNoRecovery")}</p>
    <form class="passphrase-form" onsubmit={handleChangePassphrase}>
      <div class="field">
        <label for="current-passphrase">{t("settings.currentPassphrase")}</label>
        <input
          id="current-passphrase"
          type="password"
          bind:value={currentPassphrase}
          autocomplete="current-password"
          required
          use:autofocus
        />
      </div>
      <div class="field">
        <label for="new-passphrase">{t("settings.newPassphrase")}</label>
        <input
          id="new-passphrase"
          type="password"
          bind:value={newPassphrase}
          autocomplete="new-password"
          minlength={MIN_PASSPHRASE_LENGTH}
          aria-describedby="new-passphrase-hint"
          required
        />
        <p id="new-passphrase-hint" class="hint field-hint">
          {t("settings.passphraseMinimum", { min: MIN_PASSPHRASE_LENGTH })}
        </p>
      </div>
      <div class="field">
        <label for="confirm-passphrase">{t("settings.confirmNewPassphrase")}</label>
        <input
          id="confirm-passphrase"
          type="password"
          bind:value={confirmNewPassphrase}
          autocomplete="new-password"
          required
        />
      </div>
      {#if passphraseError}<p class="error" role="alert">
          {passphraseError}
        </p>{/if}
      <div class="button-row">
        <button type="submit" disabled={changingPassphrase}>
          {changingPassphrase ? t("settings.changing") : t("common.save")}
        </button>
        <button
          type="button"
          onclick={cancelPassphraseChange}
          disabled={changingPassphrase}
        >
          {t("common.cancel")}
        </button>
      </div>
    </form>
  {/if}
</section>

<section>
  <h2>{t("settings.exportDatabase")}</h2>
  <p class="hint">{t("settings.exportDatabaseHint")}</p>
  <button type="button" onclick={handleExport} disabled={exporting}>
    {exporting ? t("settings.exporting") : t("settings.export")}
  </button>
</section>

<section>
  <h2>{t("settings.exportCsv")}</h2>
  <p class="hint">{t("settings.exportCsvHint")}</p>
  {#if accountsLoadError}
    <p class="error">{accountsLoadError}</p>
  {:else if accounts.length === 0}
    <p class="hint">{t("settings.noAccountsToExport")}</p>
  {:else}
    <div class="csv-export">
      <SearchSelect
        options={csvAccountOptions}
        value={csvAccountId}
        onChange={(id) => (csvAccountId = id)}
        placeholder={t("settings.chooseAccount")}
        searchPlaceholder={t("settings.searchAccount")}
      />
      <button
        type="button"
        onclick={handleExportCsv}
        disabled={exportingCsv || !csvAccountId}
      >
        {exportingCsv ? t("settings.exporting") : t("settings.exportCsvButton")}
      </button>
    </div>
  {/if}
</section>

<section>
  <h2>{t("settings.importDatabase")}</h2>
  <p class="hint">{t("settings.importDatabaseHint")}</p>
  {#if !importPath}
    <button type="button" onclick={handleChooseImportFile}>
      {t("settings.chooseFileToImport")}
    </button>
  {:else}
    <div class="confirm-panel">
      <p id="import-warning">
        <strong>{t("settings.importWarningStrong")}</strong>
        {t("settings.importWarningRest", { file: importFileName })}
      </p>
      <form onsubmit={handleConfirmImport}>
        <div class="field">
          <label for="import-passphrase">
            {t("settings.importPassphrase")}
          </label>
          <input
            id="import-passphrase"
            type="password"
            bind:value={importPassword}
            autocomplete="off"
            aria-describedby="import-warning"
            required
            use:autofocus
          />
        </div>
        <div class="button-row">
          <button type="submit" class="danger" disabled={importing}>
            {importing ? t("settings.importing") : t("settings.replaceDatabase")}
          </button>
          <button type="button" onclick={cancelImport} disabled={importing}>
            {t("common.cancel")}
          </button>
        </div>
      </form>
    </div>
  {/if}
</section>

<section>
  <h2>{t("settings.contactTitle")}</h2>
  <p class="hint">
    {t("settings.contactHint")}
    <span class="contact-address">{CONTACT_EMAIL}</span>.
  </p>
  <button type="button" onclick={handleContact}>{t("settings.sendEmail")}</button>
  {#if appVersion}
    <p class="hint version">Scrat {appVersion}</p>
  {/if}
</section>

<section>
  <h2>{t("settings.deleteTitle")}</h2>
  <p class="hint">{t("settings.deleteHint")}</p>
  {#if !deleteRequested}
    <button type="button" class="danger" onclick={startDelete}>
      {t("settings.deleteTitle")}
    </button>
  {:else}
    <div class="confirm-panel">
      <p id="delete-warning">
        <strong>{t("settings.deleteWarningStrong")}</strong>
        {t("settings.deleteWarningRest", { word: DELETE_CONFIRM_WORD })}
      </p>
      <form onsubmit={handleConfirmDelete}>
        <div class="field">
          <label class="visually-hidden" for="delete-confirm">
            {t("settings.deleteConfirmLabel", { word: DELETE_CONFIRM_WORD })}
          </label>
          <input
            id="delete-confirm"
            type="text"
            placeholder={DELETE_CONFIRM_WORD}
            bind:value={deleteConfirmText}
            autocomplete="off"
            aria-describedby="delete-warning"
            required
            use:autofocus
          />
        </div>
        <div class="button-row">
          <button
            type="submit"
            class="danger"
            disabled={deleting || deleteConfirmText !== DELETE_CONFIRM_WORD}
          >
            {deleting ? t("settings.deleting") : t("settings.deletePermanently")}
          </button>
          <button type="button" onclick={cancelDelete} disabled={deleting}>
            {t("common.cancel")}
          </button>
        </div>
      </form>
    </div>
  {/if}
</section>

<style>
  h1 {
    margin-top: 0;
  }

  section {
    max-width: 32rem;
    margin-bottom: 2rem;
  }

  h2 {
    font-size: 1.05rem;
    margin-bottom: 0.4rem;
  }

  .hint {
    opacity: 0.8;
    font-size: 0.9rem;
    margin-top: 0;
  }

  .error {
    color: var(--color-danger);
  }

  /* The address is deliberately selectable text as well as a button — a user
     without a configured mail client still needs to be able to copy it. */
  .contact-address {
    user-select: text;
    white-space: nowrap;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }

  .version {
    margin-top: 0.6rem;
    margin-bottom: 0;
    font-size: 0.8rem;
    opacity: 0.6;
  }

  .csv-export {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin-bottom: 0.6rem;
  }

  form {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .passphrase-form {
    flex-direction: column;
    align-items: stretch;
    max-width: 20rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    flex: 1;
    min-width: 0;
  }

  .field label {
    font-size: 0.85rem;
    opacity: 0.9;
  }

  .field-hint {
    margin: 0;
    font-size: 0.8rem;
  }

  /* Available to screen readers, removed from the visual layout — for
     controls a neighbouring heading or placeholder already names on screen,
     which still leaves them unnamed in the accessibility tree. */
  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .button-row {
    display: flex;
    gap: 0.6rem;
  }

  select,
  input,
  button {
    border-radius: 6px;
    border: 1px solid var(--color-shade-3);
    padding: 0.45rem 0.9rem;
    font-size: 0.9rem;
    font-family: inherit;
  }

  select,
  input {
    background-color: var(--color-shade-2);
    color: inherit;
  }

  button {
    cursor: pointer;
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
    border: none;
  }

  button.danger {
    background-color: var(--color-danger);
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .confirm-panel {
    padding: 1rem;
    border-radius: 10px;
    background-color: color-mix(in srgb, var(--color-danger) 15%, transparent);
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .confirm-panel p {
    margin: 0;
  }
</style>
