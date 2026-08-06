<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { save, open } from "@tauri-apps/plugin-dialog";
  import SearchSelect from "$lib/SearchSelect.svelte";
  import { api, type AccountDto } from "$lib/api";
  import { clearPageCache } from "$lib/pageCache";
  import { toast } from "$lib/toasts.svelte";
  import { session } from "$lib/session.svelte";

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

  const AUTO_LOCK_OPTIONS = [
    { minutes: 1, label: "1 minute" },
    { minutes: 10, label: "10 minutes" },
    { minutes: 60, label: "1 hour" },
    { minutes: 0, label: "Never" },
  ];

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
    try {
      currentCurrency = await api.getCurrency();
      selectedCurrency = currentCurrency;
    } catch (e) {
      loadError = String(e);
    } finally {
      loadingCurrency = false;
    }

    try {
      currentAutoLockMinutes = await api.getAutoLockMinutes();
      selectedAutoLockMinutes = currentAutoLockMinutes;
    } catch (e) {
      autoLockLoadError = String(e);
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
      accountsLoadError = String(e);
    }
  });

  async function handleSaveCurrency(event: Event) {
    event.preventDefault();
    try {
      await api.setCurrency(selectedCurrency);
      currentCurrency = selectedCurrency;
      toast.success(`Currency set to ${selectedCurrency}.`);
    } catch (e) {
      toast.error(String(e));
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
          ?.label ?? `${selectedAutoLockMinutes} minutes`;
      toast.success(`Auto-lock set to ${label.toLowerCase()}.`);
    } catch (e) {
      toast.error(String(e));
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
        defaultPath: "scrat-export.db",
        filters: [{ name: "Scrat Database", extensions: ["db"] }],
      });
      if (!destination) return;
      exporting = true;
      await api.exportDatabase(destination);
      toast.success(`Exported to ${destination}`);
    } catch (e) {
      toast.error(String(e));
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
      toast.success(`Exported to ${destination}`);
    } catch (e) {
      toast.error(String(e));
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
      toast.error(String(e));
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
      toast.success("Database imported.");
      await goto("/overview");
    } catch (e) {
      toast.error(String(e));
    } finally {
      importing = false;
    }
  }

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
    if (newPassphrase.length < 8) {
      passphraseError = "New passphrase must be at least 8 characters.";
      return;
    }
    if (newPassphrase !== confirmNewPassphrase) {
      passphraseError = "New passphrases do not match.";
      return;
    }
    changingPassphrase = true;
    try {
      await api.changePassphrase(currentPassphrase, newPassphrase);
      toast.success("Passphrase changed.");
      cancelPassphraseChange();
    } catch (e) {
      passphraseError = String(e);
    } finally {
      changingPassphrase = false;
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
      toast.success("Your data has been deleted.");
      await goto("/");
    } catch (e) {
      toast.error(String(e));
      deleting = false;
    }
  }
</script>

<h1>Settings</h1>

<section>
  <h2>Currency</h2>
  <p class="hint">Relabels amounts only — past transactions aren't converted.</p>
  {#if loadingCurrency}
    <p>Loading…</p>
  {:else if loadError}
    <p class="error" role="alert">{loadError}</p>
  {:else}
    <form onsubmit={handleSaveCurrency}>
      <label class="visually-hidden" for="currency">Currency</label>
      <select id="currency" bind:value={selectedCurrency}>
        {#each currencyOptions as code (code)}
          <option value={code}>{code}</option>
        {/each}
      </select>
      <button type="submit" disabled={selectedCurrency === currentCurrency}
        >Save</button
      >
    </form>
  {/if}
</section>

<section>
  <h2>Auto-lock</h2>
  <p class="hint">
    Locks the app and asks for your passphrase again after this much time
    without mouse or keyboard activity.
  </p>
  {#if loadingAutoLock}
    <p>Loading…</p>
  {:else if autoLockLoadError}
    <p class="error" role="alert">{autoLockLoadError}</p>
  {:else}
    <form onsubmit={handleSaveAutoLock}>
      <label class="visually-hidden" for="auto-lock">Auto-lock</label>
      <select id="auto-lock" bind:value={selectedAutoLockMinutes}>
        {#each AUTO_LOCK_OPTIONS as option (option.minutes)}
          <option value={option.minutes}>{option.label}</option>
        {/each}
      </select>
      <button
        type="submit"
        disabled={selectedAutoLockMinutes === currentAutoLockMinutes}
      >
        Save
      </button>
    </form>
  {/if}
</section>

<section>
  <h2>Passphrase</h2>
  {#if !showPassphraseForm}
    <p class="hint">Change the passphrase used to encrypt your database.</p>
    <button type="button" onclick={startPassphraseChange}>
      Change passphrase
    </button>
  {:else}
    <p class="hint">
      No recovery — if you lose the new passphrase, your data is unreadable.
    </p>
    <form class="passphrase-form" onsubmit={handleChangePassphrase}>
      <div class="field">
        <label for="current-passphrase">Current passphrase</label>
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
        <label for="new-passphrase">New passphrase</label>
        <input
          id="new-passphrase"
          type="password"
          bind:value={newPassphrase}
          autocomplete="new-password"
          minlength="8"
          aria-describedby="new-passphrase-hint"
          required
        />
        <p id="new-passphrase-hint" class="hint field-hint">
          At least 8 characters.
        </p>
      </div>
      <div class="field">
        <label for="confirm-passphrase">Confirm new passphrase</label>
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
          {changingPassphrase ? "Changing…" : "Save"}
        </button>
        <button
          type="button"
          onclick={cancelPassphraseChange}
          disabled={changingPassphrase}
        >
          Cancel
        </button>
      </div>
    </form>
  {/if}
</section>

<section>
  <h2>Export database</h2>
  <p class="hint">Saves an encrypted copy of your database file.</p>
  <button type="button" onclick={handleExport} disabled={exporting}>
    {exporting ? "Exporting…" : "Export"}
  </button>
</section>

<section>
  <h2>Export transaction CSV</h2>
  <p class="hint">
    Saves one account's transactions as a CSV file, readable outside Scrat.
  </p>
  {#if accountsLoadError}
    <p class="error">{accountsLoadError}</p>
  {:else if accounts.length === 0}
    <p class="hint">Add an account first — there's nothing to export yet.</p>
  {:else}
    <div class="csv-export">
      <SearchSelect
        options={csvAccountOptions}
        value={csvAccountId}
        onChange={(id) => (csvAccountId = id)}
        placeholder="Choose an account…"
        searchPlaceholder="Search account…"
      />
      <button
        type="button"
        onclick={handleExportCsv}
        disabled={exportingCsv || !csvAccountId}
      >
        {exportingCsv ? "Exporting…" : "Export CSV"}
      </button>
    </div>
  {/if}
</section>

<section>
  <h2>Import database</h2>
  <p class="hint">Replaces everything in Scrat with another encrypted database file.</p>
  {#if !importPath}
    <button type="button" onclick={handleChooseImportFile}>
      Choose file to import
    </button>
  {:else}
    <div class="confirm-panel">
      <p id="import-warning">
        <strong>This will permanently replace your current database</strong>
        with <code>{importFileName}</code>. This cannot be undone — export
        your current database first if you want to keep a copy.
      </p>
      <form onsubmit={handleConfirmImport}>
        <div class="field">
          <label for="import-passphrase">Passphrase for the imported file</label
          >
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
            {importing ? "Importing…" : "Replace database"}
          </button>
          <button type="button" onclick={cancelImport} disabled={importing}>
            Cancel
          </button>
        </div>
      </form>
    </div>
  {/if}
</section>

<section>
  <h2>Delete my data</h2>
  <p class="hint">Permanently deletes your local database. No backup is made.</p>
  {#if !deleteRequested}
    <button type="button" class="danger" onclick={startDelete}>
      Delete my data
    </button>
  {:else}
    <div class="confirm-panel">
      <p id="delete-warning">
        <strong>This will permanently delete all of your data.</strong>
        There is no undo and no backup. Type
        <code>{DELETE_CONFIRM_WORD}</code> below to confirm.
      </p>
      <form onsubmit={handleConfirmDelete}>
        <div class="field">
          <label class="visually-hidden" for="delete-confirm">
            Type {DELETE_CONFIRM_WORD} to confirm
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
            {deleting ? "Deleting…" : "Permanently delete"}
          </button>
          <button type="button" onclick={cancelDelete} disabled={deleting}>
            Cancel
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
