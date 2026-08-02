<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { save, open } from "@tauri-apps/plugin-dialog";
  import { api } from "$lib/api";
  import { clearPageCache } from "$lib/pageCache";
  import { toast } from "$lib/toasts.svelte";

  const COMMON_CURRENCIES = ["USD", "EUR", "GBP", "CAD", "AUD", "CHF", "JPY"];

  let currentCurrency = $state("");
  let selectedCurrency = $state("");
  let loadError = $state("");
  let loadingCurrency = $state(true);

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

  async function handleExportCsv() {
    try {
      const destination = await save({
        defaultPath: timestampedFileName("scrat-transactions", "csv"),
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!destination) return;
      exportingCsv = true;
      await api.exportTransactionsCsv(destination);
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
    <p class="error">{loadError}</p>
  {:else}
    <form onsubmit={handleSaveCurrency}>
      <select bind:value={selectedCurrency}>
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
      <input
        type="password"
        placeholder="Current passphrase"
        bind:value={currentPassphrase}
        autocomplete="current-password"
        required
      />
      <input
        type="password"
        placeholder="New passphrase"
        bind:value={newPassphrase}
        autocomplete="new-password"
        required
      />
      <input
        type="password"
        placeholder="Confirm new passphrase"
        bind:value={confirmNewPassphrase}
        autocomplete="new-password"
        required
      />
      {#if passphraseError}<p class="error">{passphraseError}</p>{/if}
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
    Saves every transaction as a CSV file, readable outside Scrat.
  </p>
  <button type="button" onclick={handleExportCsv} disabled={exportingCsv}>
    {exportingCsv ? "Exporting…" : "Export CSV"}
  </button>
</section>

<section>
  <h2>Import database</h2>
  <p class="hint">Replaces everything in Scrat with another encrypted database file.</p>
  {#if !importPath}
    <button type="button" onclick={handleChooseImportFile}>
      Choose file to import
    </button>
  {:else}
    <div class="import-warning-panel">
      <p>
        <strong>This will permanently replace your current database</strong>
        with <code>{importFileName}</code>. This cannot be undone — export
        your current database first if you want to keep a copy.
      </p>
      <form onsubmit={handleConfirmImport}>
        <input
          type="password"
          placeholder="Passphrase for the imported file"
          bind:value={importPassword}
          autocomplete="off"
          required
        />
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
    <div class="import-warning-panel">
      <p>
        <strong>This will permanently delete all of your data.</strong>
        There is no undo and no backup. Type
        <code>{DELETE_CONFIRM_WORD}</code> below to confirm.
      </p>
      <form onsubmit={handleConfirmDelete}>
        <input
          type="text"
          placeholder={DELETE_CONFIRM_WORD}
          bind:value={deleteConfirmText}
          autocomplete="off"
          required
        />
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

  .import-warning-panel {
    padding: 1rem;
    border-radius: 10px;
    background-color: color-mix(in srgb, var(--color-danger) 15%, transparent);
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .import-warning-panel p {
    margin: 0;
  }
</style>
