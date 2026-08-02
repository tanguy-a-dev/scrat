<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { save, open } from "@tauri-apps/plugin-dialog";
  import { api } from "$lib/api";
  import { toast } from "$lib/toasts.svelte";

  const COMMON_CURRENCIES = ["USD", "EUR", "GBP", "CAD", "AUD", "CHF", "JPY"];

  let currentCurrency = $state("");
  let selectedCurrency = $state("");
  let loadError = $state("");
  let loadingCurrency = $state(true);

  let exporting = $state(false);
  let exportingCsv = $state(false);

  let importPath = $state<string | null>(null);
  let importPassword = $state("");
  let importing = $state(false);
  let importFileName = $derived(importPath?.split(/[\\/]/).pop() ?? "");

  let onePasswordRef = $state("");
  let savedOnePasswordRef = $state<string | null>(null);
  let testingOnePassword = $state(false);

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
      savedOnePasswordRef = await api.get1PasswordReference();
      onePasswordRef = savedOnePasswordRef ?? "";
    } catch {
      // Not fatal — the rest of settings still works without it.
    }
  });

  /** Verifies before saving, so a typo surfaces here rather than at the
   * next launch's unlock screen. */
  async function handleSaveOnePassword(event: Event) {
    event.preventDefault();
    const reference = onePasswordRef.trim();
    testingOnePassword = true;
    try {
      await api.test1PasswordReference(reference);
      await api.set1PasswordReference(reference);
      savedOnePasswordRef = reference;
      toast.success("1Password unlock is set up. It takes effect at next launch.");
    } catch (e) {
      toast.error(String(e));
    } finally {
      testingOnePassword = false;
    }
  }

  async function handleDisableOnePassword() {
    try {
      await api.set1PasswordReference(null);
      savedOnePasswordRef = null;
      onePasswordRef = "";
      toast.success("1Password unlock turned off.");
    } catch (e) {
      toast.error(String(e));
    }
  }

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

  async function handleExport() {
    exporting = true;
    try {
      const destination = await save({
        defaultPath: "scrat-export.db",
        filters: [{ name: "Scrat Database", extensions: ["db"] }],
      });
      if (!destination) return;
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
    exportingCsv = true;
    try {
      const destination = await save({
        defaultPath: timestampedFileName("scrat-transactions", "csv"),
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!destination) return;
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
      toast.success("Database imported.");
      await goto("/overview");
    } catch (e) {
      toast.error(String(e));
    } finally {
      importing = false;
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
  <h2>Set currency</h2>
  <p class="hint">
    Changing this only relabels amounts from now on — past transactions are
    not converted, since their original numbers don't change.
  </p>
  {#if loadingCurrency}
    <p>Loading…</p>
  {:else if loadError}
    <p class="error">{loadError}</p>
  {:else}
    <form onsubmit={handleSaveCurrency}>
      <select bind:value={selectedCurrency}>
        {#each COMMON_CURRENCIES as code (code)}
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
  <h2>Unlock with 1Password</h2>
  <p class="hint">
    Scrat can read your passphrase from your own 1Password vault at launch,
    unlocked by Touch ID. Only the reference below is stored on this machine —
    never the passphrase itself. Requires the 1Password CLI (<code
      >brew install 1password-cli</code
    >) and Settings → Developer → "Integrate with 1Password CLI" enabled in
    the 1Password app.
  </p>
  <form onsubmit={handleSaveOnePassword}>
    <input
      type="text"
      bind:value={onePasswordRef}
      placeholder="op://Personal/Scrat/password"
      spellcheck="false"
      autocomplete="off"
    />
    <button
      type="submit"
      disabled={testingOnePassword || !onePasswordRef.trim()}
    >
      {testingOnePassword ? "Checking…" : "Verify and save"}
    </button>
    {#if savedOnePasswordRef}
      <button
        type="button"
        onclick={handleDisableOnePassword}
        disabled={testingOnePassword}>Turn off</button
      >
    {/if}
  </form>
</section>

<section>
  <h2>Export database</h2>
  <p class="hint">
    Saves a copy of your encrypted database file — the copy stays encrypted,
    so exporting never weakens your data's protection.
  </p>
  <button type="button" onclick={handleExport} disabled={exporting}>
    {exporting ? "Exporting…" : "Export"}
  </button>
</section>

<section>
  <h2>Export transaction CSV</h2>
  <p class="hint">
    Saves every transaction as a plain-text CSV file, with ";" as the
    separator — account and category names are included so the file is
    readable outside Scrat.
  </p>
  <button type="button" onclick={handleExportCsv} disabled={exportingCsv}>
    {exportingCsv ? "Exporting…" : "Export CSV"}
  </button>
</section>

<section>
  <h2>Import database</h2>
  <p class="hint">
    Replaces everything in Scrat with the contents of an encrypted database
    file. The file stays encrypted throughout — you'll need its passphrase to
    open it.
  </p>
  {#if !importPath}
    <button type="button" onclick={handleChooseImportFile}>
      Choose file to import
    </button>
  {:else}
    <div class="import-warning-panel">
      <p>
        <strong>This will permanently replace your current database</strong>
        with <code>{importFileName}</code>. Everything currently in Scrat
        will be gone — this cannot be undone. Export your current database
        first if you want to keep a copy.
      </p>
      <form onsubmit={handleConfirmImport}>
        <input
          type="password"
          placeholder="Passphrase for the imported file"
          bind:value={importPassword}
          autocomplete="off"
          required
        />
        <button type="submit" class="danger" disabled={importing}>
          {importing ? "Importing…" : "Replace database"}
        </button>
        <button type="button" onclick={cancelImport} disabled={importing}>
          Cancel
        </button>
      </form>
    </div>
  {/if}
</section>

<section>
  <h2>Suggest categories</h2>
  <p class="hint">
    Scrat looks at how you've categorized similar transactions before and
    suggests a category automatically — while you type a description on the
    Transactions page, and per row when importing a CSV. It's a local
    frequency lookup only: no machine learning, nothing ever leaves your
    computer.
  </p>
</section>

<section>
  <h2>Delete my data</h2>
  <p class="hint">
    Permanently deletes your local encrypted database — every account,
    category, and transaction. This cannot be undone, and no backup is made.
    Export your database first if you want to keep a copy.
  </p>
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
