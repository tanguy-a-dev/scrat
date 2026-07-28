<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    buildCategoryOptions,
    formatMinorUnits,
    parseToMinorUnits,
    computeRange,
    todayIsoDate,
    type AccountDto,
    type CategoryDto,
    type TransactionDto,
    type RangeMode,
  } from "$lib/api";
  import ImportCsvDialog from "$lib/ImportCsvDialog.svelte";

  let showImportDialog = $state(false);

  let accounts = $state<AccountDto[]>([]);
  let categories = $state<CategoryDto[]>([]);
  let transactions = $state<TransactionDto[]>([]);
  let loading = $state(true);
  let error = $state("");

  let rangeMode = $state<RangeMode>("month");
  let customStart = $state(todayIsoDate());
  let customEnd = $state(todayIsoDate());
  let currentRange = $state({ start: todayIsoDate(), end: todayIsoDate() });

  let pendingBulkDelete = $state(false);
  let bulkDeleting = $state(false);

  type SortField = "date" | "amount" | "source" | "category";
  let sortField = $state<SortField>("date");
  let sortDir = $state<"asc" | "desc">("desc");

  let formDate = $state(todayIsoDate());
  let formAmount = $state("");
  let formSource = $state("");
  let formCategoryId = $state("");
  let formAccountId = $state("");

  onMount(load);

  async function load() {
    loading = true;
    error = "";
    try {
      const range = computeRange(rangeMode, {
        start: customStart,
        end: customEnd,
      });
      currentRange = range;
      const [a, c, t] = await Promise.all([
        api.listAccounts(),
        api.listCategories(),
        api.listTransactions(range.start, range.end),
      ]);
      accounts = a;
      categories = c;
      transactions = t;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function setRange(mode: RangeMode) {
    rangeMode = mode;
    load();
  }

  function categoryName(id: string): string {
    return categories.find((c) => c.id === id)?.name ?? "—";
  }

  function accountName(id: string): string {
    return accounts.find((a) => a.id === id)?.name ?? "—";
  }

  let categoryOptions = $derived(buildCategoryOptions(categories));

  async function handleSourceBlur() {
    const source = formSource.trim();
    if (!source) return;
    if (!formAccountId) {
      try {
        const suggested = await api.suggestAccountForSource(source);
        if (suggested) formAccountId = suggested;
      } catch {
        // best-effort suggestion only
      }
    }
    if (!formCategoryId) {
      try {
        const suggested = await api.suggestCategoryForSource(source);
        if (suggested) formCategoryId = suggested;
      } catch {
        // best-effort suggestion only
      }
    }
  }

  async function handleCreate(event: Event) {
    event.preventDefault();
    error = "";
    const minorUnits = parseToMinorUnits(formAmount);
    if (minorUnits === null || minorUnits === 0) {
      error =
        "Amount must be a non-zero number (negative for expense, positive for income).";
      return;
    }
    if (!formCategoryId || !formAccountId) {
      error = "Choose a category and an account.";
      return;
    }
    try {
      await api.createTransaction(
        formDate,
        minorUnits,
        formSource.trim(),
        formCategoryId,
        formAccountId,
      );
      formAmount = "";
      formSource = "";
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function handleDelete(id: string) {
    error = "";
    try {
      await api.deleteTransaction(id);
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function confirmBulkDelete() {
    error = "";
    bulkDeleting = true;
    try {
      await api.deleteTransactionsInRange(currentRange.start, currentRange.end);
      pendingBulkDelete = false;
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      bulkDeleting = false;
    }
  }

  function toggleSort(field: SortField) {
    if (sortField === field) {
      sortDir = sortDir === "asc" ? "desc" : "asc";
    } else {
      sortField = field;
      sortDir = "desc";
    }
  }

  function sortTransactions(list: TransactionDto[]): TransactionDto[] {
    return [...list].sort((a, b) => {
      let cmp = 0;
      if (sortField === "date") cmp = a.date.localeCompare(b.date);
      else if (sortField === "amount")
        cmp = a.amount_minor_units - b.amount_minor_units;
      else if (sortField === "source") cmp = a.source.localeCompare(b.source);
      else
        cmp = categoryName(a.category_id).localeCompare(
          categoryName(b.category_id),
        );
      return sortDir === "asc" ? cmp : -cmp;
    });
  }

  let expenses = $derived(
    sortTransactions(transactions.filter((t) => t.amount_minor_units < 0)),
  );
  let income = $derived(
    sortTransactions(transactions.filter((t) => t.amount_minor_units > 0)),
  );
</script>

{#snippet list(items: TransactionDto[])}
  {#if items.length === 0}
    <p class="empty">No transactions.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th
            ><button type="button" onclick={() => toggleSort("date")}
              >Date</button
            ></th
          >
          <th
            ><button type="button" onclick={() => toggleSort("amount")}
              >Amount</button
            ></th
          >
          <th
            ><button type="button" onclick={() => toggleSort("source")}
              >Source</button
            ></th
          >
          <th
            ><button type="button" onclick={() => toggleSort("category")}
              >Category</button
            ></th
          >
          <th>Account</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each items as t (t.id)}
          <tr>
            <td>{t.date}</td>
            <td>{formatMinorUnits(t.amount_minor_units)} {t.currency}</td>
            <td>{t.source}</td>
            <td>{categoryName(t.category_id)}</td>
            <td>{accountName(t.account_id)}</td>
            <td>
              <button
                type="button"
                class="danger"
                onclick={() => handleDelete(t.id)}>Delete</button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
{/snippet}

<h1>Transactions</h1>

{#if error}<p class="error">{error}</p>{/if}

<div class="range-bar">
  <div class="range-buttons">
    <button
      type="button"
      class:active={rangeMode === "month"}
      onclick={() => setRange("month")}>Month</button
    >
    <button
      type="button"
      class:active={rangeMode === "year"}
      onclick={() => setRange("year")}>Year</button
    >
    <button
      type="button"
      class:active={rangeMode === "all"}
      onclick={() => setRange("all")}>All Time</button
    >
    <button
      type="button"
      class:active={rangeMode === "custom"}
      onclick={() => setRange("custom")}>Set Dates</button
    >
  </div>
  {#if rangeMode === "custom"}
    <input type="date" bind:value={customStart} onchange={load} />
    <span>to</span>
    <input type="date" bind:value={customEnd} onchange={load} />
  {/if}
  <button
    type="button"
    class="danger bulk-delete-button"
    disabled={transactions.length === 0}
    onclick={() => (pendingBulkDelete = true)}
  >
    Delete all in range
  </button>
  <button
    type="button"
    class="import-button"
    onclick={() => (showImportDialog = true)}>Import CSV</button
  >
</div>

{#if pendingBulkDelete}
  <div class="bulk-delete-panel">
    <p>
      Delete all {transactions.length} transaction{transactions.length === 1
        ? ""
        : "s"} between {currentRange.start} and {currentRange.end}? This cannot
      be undone.
    </p>
    <button
      type="button"
      class="danger"
      disabled={bulkDeleting}
      onclick={confirmBulkDelete}
    >
      {bulkDeleting ? "Deleting…" : `Delete ${transactions.length} transaction${transactions.length === 1 ? "" : "s"}`}
    </button>
    <button
      type="button"
      onclick={() => (pendingBulkDelete = false)}
      disabled={bulkDeleting}
    >
      Cancel
    </button>
  </div>
{/if}

{#if showImportDialog}
  <ImportCsvDialog
    {accounts}
    {categories}
    onImported={load}
    onClose={() => (showImportDialog = false)}
  />
{/if}

<form class="create-form" onsubmit={handleCreate}>
  <input type="date" bind:value={formDate} required />
  <input
    type="number"
    step="0.01"
    placeholder="Amount (− expense / + income)"
    bind:value={formAmount}
    required
  />
  <input
    placeholder="Source"
    bind:value={formSource}
    onblur={handleSourceBlur}
    required
  />
  <select bind:value={formCategoryId} required>
    <option value="" disabled selected>Category…</option>
    {#each categoryOptions as c (c.id)}
      <option value={c.id}>{c.label}</option>
    {/each}
  </select>
  <select bind:value={formAccountId} required>
    <option value="" disabled selected>Account…</option>
    {#each accounts.filter((a) => a.status === "active") as a (a.id)}
      <option value={a.id}>{a.name}</option>
    {/each}
  </select>
  <button type="submit">Add transaction</button>
</form>

{#if loading}
  <p>Loading…</p>
{:else}
  <div class="lists">
    <section>
      <h2>Expenses</h2>
      {@render list(expenses)}
    </section>
    <section>
      <h2>Income</h2>
      {@render list(income)}
    </section>
  </div>
{/if}

<style>
  h1 {
    margin-top: 0;
  }

  .error {
    color: #d33;
  }

  .empty {
    opacity: 0.75;
  }

  .range-bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 1rem;
  }

  .import-button {
    background-color: #396cd8;
    color: white;
    border: none;
    cursor: pointer;
  }

  .bulk-delete-button {
    margin-left: auto;
  }

  .bulk-delete-panel {
    margin-bottom: 1.5rem;
    padding: 1rem;
    border-radius: 10px;
    background-color: rgba(179, 38, 30, 0.1);
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    max-width: 32rem;
  }

  .range-buttons {
    display: flex;
    gap: 0.4rem;
  }

  .range-buttons button {
    background-color: rgba(0, 0, 0, 0.06);
    color: inherit;
  }

  .range-buttons button.active {
    background-color: #396cd8;
    color: white;
  }

  .create-form {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-bottom: 2rem;
  }

  input,
  select,
  button {
    border-radius: 6px;
    border: 1px solid rgba(0, 0, 0, 0.15);
    padding: 0.45rem 0.7rem;
    font-size: 0.9rem;
    font-family: inherit;
  }

  .create-form button,
  .range-buttons button {
    cursor: pointer;
    border: none;
  }

  .create-form button {
    background-color: #396cd8;
    color: white;
  }

  .lists {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2rem;
  }

  h2 {
    font-size: 1.1rem;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }

  th {
    text-align: left;
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid rgba(0, 0, 0, 0.15);
  }

  th button {
    border: none;
    background: none;
    padding: 0;
    font-weight: 600;
    font-size: 0.85rem;
    cursor: pointer;
    color: inherit;
  }

  td {
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid rgba(0, 0, 0, 0.06);
  }

  button.danger {
    background-color: #b3261e;
    color: white;
    border: none;
    padding: 0.3rem 0.55rem;
    font-size: 0.8rem;
  }

  @media (prefers-color-scheme: dark) {
    input,
    select {
      background-color: rgba(255, 255, 255, 0.06);
      border-color: rgba(255, 255, 255, 0.15);
      color: inherit;
    }

    .range-buttons button:not(.active) {
      background-color: rgba(255, 255, 255, 0.08);
    }

    th {
      border-bottom-color: rgba(255, 255, 255, 0.15);
    }

    td {
      border-bottom-color: rgba(255, 255, 255, 0.08);
    }
  }

  @media (max-width: 900px) {
    .lists {
      grid-template-columns: 1fr;
    }
  }
</style>
