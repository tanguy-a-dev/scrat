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
    suggests a category automatically — while you type a source on the
    Transactions page, and per row when importing a CSV. It's a local
    frequency lookup only: no machine learning, nothing ever leaves your
    computer.
  </p>
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
