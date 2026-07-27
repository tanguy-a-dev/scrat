<script lang="ts">
  import { onMount } from "svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import { api } from "$lib/api";

  const COMMON_CURRENCIES = ["USD", "EUR", "GBP", "CAD", "AUD", "CHF", "JPY"];

  let currentCurrency = $state("");
  let selectedCurrency = $state("");
  let currencyError = $state("");
  let currencySaved = $state(false);
  let loadingCurrency = $state(true);

  let exportError = $state("");
  let exportSuccess = $state("");
  let exporting = $state(false);

  onMount(async () => {
    try {
      currentCurrency = await api.getCurrency();
      selectedCurrency = currentCurrency;
    } catch (e) {
      currencyError = String(e);
    } finally {
      loadingCurrency = false;
    }
  });

  async function handleSaveCurrency(event: Event) {
    event.preventDefault();
    currencyError = "";
    currencySaved = false;
    try {
      await api.setCurrency(selectedCurrency);
      currentCurrency = selectedCurrency;
      currencySaved = true;
    } catch (e) {
      currencyError = String(e);
    }
  }

  async function handleExport() {
    exportError = "";
    exportSuccess = "";
    exporting = true;
    try {
      const destination = await save({
        defaultPath: "scrat-export.db",
        filters: [{ name: "Scrat Database", extensions: ["db"] }],
      });
      if (!destination) return;
      await api.exportDatabase(destination);
      exportSuccess = `Exported to ${destination}`;
    } catch (e) {
      exportError = String(e);
    } finally {
      exporting = false;
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
      {#if currencySaved}<span class="success">Saved.</span>{/if}
    </form>
    {#if currencyError}<p class="error">{currencyError}</p>{/if}
  {/if}
</section>

<section>
  <h2>Export database</h2>
  <p class="hint">
    Saves a copy of your encrypted database file — the copy stays encrypted,
    so exporting never weakens your data's protection.
  </p>
  <button type="button" onclick={handleExport} disabled={exporting}>
    {exporting ? "Exporting…" : "Choose location & export"}
  </button>
  {#if exportSuccess}<p class="success">{exportSuccess}</p>{/if}
  {#if exportError}<p class="error">{exportError}</p>{/if}
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
    color: #d33;
  }

  .success {
    color: #2a9d5c;
  }

  form {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  select,
  button {
    border-radius: 6px;
    border: 1px solid rgba(0, 0, 0, 0.15);
    padding: 0.45rem 0.9rem;
    font-size: 0.9rem;
    font-family: inherit;
  }

  button {
    cursor: pointer;
    background-color: #396cd8;
    color: white;
    border: none;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  @media (prefers-color-scheme: dark) {
    select {
      background-color: rgba(255, 255, 255, 0.06);
      border-color: rgba(255, 255, 255, 0.15);
      color: inherit;
    }
  }
</style>
