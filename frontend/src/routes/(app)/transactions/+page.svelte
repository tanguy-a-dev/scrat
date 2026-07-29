<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { replaceState } from "$app/navigation";
  import {
    api,
    buildCategoryOptions,
    formatCurrency,
    parseToMinorUnits,
    computeRange,
    todayIsoDate,
    type AccountDto,
    type CategoryDto,
    type TransactionDto,
    type RangeMode,
  } from "$lib/api";
  import ImportCsvDialog from "$lib/ImportCsvDialog.svelte";
  import DeleteButton from "$lib/DeleteButton.svelte";
  import { toast } from "$lib/toasts.svelte";
  import { FileUp, Plus } from "@lucide/svelte";

  let showImportDialog = $state(false);
  let showAddForm = $state(false);

  // The Cmd/Ctrl+K command palette navigates here with ?action=... to
  // trigger these directly instead of just landing on the page.
  $effect(() => {
    const action = page.url.searchParams.get("action");
    if (action === "add-transaction") {
      showAddForm = true;
      replaceState(page.url.pathname, {});
    } else if (action === "import-csv") {
      showImportDialog = true;
      replaceState(page.url.pathname, {});
    }
  });

  let accounts = $state<AccountDto[]>([]);
  let categories = $state<CategoryDto[]>([]);
  let transactions = $state<TransactionDto[]>([]);
  let loading = $state(true);
  let error = $state("");

  let rangeMode = $state<RangeMode>("month");
  let customStart = $state(todayIsoDate());
  let customEnd = $state(todayIsoDate());
  let currentRange = $state({ start: todayIsoDate(), end: todayIsoDate() });

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
    const minorUnits = parseToMinorUnits(formAmount);
    if (minorUnits === null || minorUnits === 0) {
      toast.error(
        "Amount must be a non-zero number (negative for expense, positive for income).",
      );
      return;
    }
    if (!formCategoryId || !formAccountId) {
      toast.error("Choose a category and an account.");
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
      toast.error(String(e));
    }
  }

  async function handleCategoryChange(t: TransactionDto, categoryId: string) {
    if (categoryId === t.category_id) return;
    try {
      await api.setTransactionCategory(t.id, categoryId);
      await load();
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function handleDelete(id: string) {
    try {
      await api.deleteTransaction(id);
      await load();
      toast.success("Transaction deleted.");
    } catch (e) {
      toast.error(String(e));
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
            <td>{formatCurrency(t.amount_minor_units, t.currency)}</td>
            <td>{t.source}</td>
            <td>
              <select
                class="category-select"
                value={t.category_id}
                onchange={(e) =>
                  handleCategoryChange(t, e.currentTarget.value)}
              >
                {#each categoryOptions as c (c.id)}
                  <option value={c.id}>{c.label}</option>
                {/each}
              </select>
            </td>
            <td>{accountName(t.account_id)}</td>
            <td>
              <DeleteButton
                label="Delete transaction"
                onConfirm={() => handleDelete(t.id)}
              />
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
  <div class="actions">
    <button
      type="button"
      class="icon-button add-button"
      aria-label="Add transaction"
      title="Add transaction"
      onclick={() => (showAddForm = !showAddForm)}
    >
      <Plus size={18} />
    </button>
    <button
      type="button"
      class="icon-button import-button"
      aria-label="Import CSV"
      title="Import CSV"
      onclick={() => (showImportDialog = true)}
    >
      <FileUp size={18} />
    </button>
  </div>
</div>

{#if showImportDialog}
  <ImportCsvDialog
    {accounts}
    {categories}
    onImported={load}
    onClose={() => (showImportDialog = false)}
  />
{/if}

{#if showAddForm}
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
      {#each accounts as a (a.id)}
        <option value={a.id}>{a.name}</option>
      {/each}
    </select>
    <button type="submit">Save transaction</button>
  </form>
{/if}

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
    color: var(--color-danger);
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

  .actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .add-button {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
  }

  .import-button {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
  }

  .range-buttons {
    display: flex;
    gap: 0.4rem;
  }

  .range-buttons button {
    background-color: var(--color-shade-3);
    color: inherit;
  }

  .range-buttons button.active {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
  }

  .create-form {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-bottom: 2rem;
  }

  input,
  select,
  button:not(.icon-button) {
    border-radius: 6px;
    border: 1px solid var(--color-shade-3);
    padding: 0.45rem 0.7rem;
    font-size: 0.9rem;
    font-family: inherit;
  }

  input,
  select {
    background-color: var(--color-shade-2);
    color: inherit;
  }

  .create-form button,
  .range-buttons button {
    cursor: pointer;
    border: none;
  }

  .create-form button {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
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
    border-bottom: 1px solid var(--color-shade-3);
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
    border-bottom: 1px solid var(--color-shade-2);
  }

  .category-select {
    border-radius: 4px;
    border: 1px solid var(--color-shade-3);
    background-color: var(--color-shade-2);
    color: inherit;
    padding: 0.2rem 0.4rem;
    font-size: 0.85rem;
    font-family: inherit;
    max-width: 11rem;
  }

  @media (max-width: 900px) {
    .lists {
      grid-template-columns: 1fr;
    }
  }
</style>
