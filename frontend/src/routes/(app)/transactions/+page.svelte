<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
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
  import CategorySelect from "$lib/CategorySelect.svelte";
  import FilterPopover from "$lib/FilterPopover.svelte";
  import { toast } from "$lib/toasts.svelte";
  import { ArrowUp, Check, FileUp, Minus, Pencil, Plus, Search } from "@lucide/svelte";

  function autofocus(node: HTMLElement) {
    node.focus();
  }

  /** Keeps a checkbox's native `indeterminate` visual state in sync — there
   * is no HTML attribute for it, only the DOM property. Applied to the real
   * (visually hidden) input so screen readers still get it; the visible box
   * is drawn separately in the `checkbox` snippet below. */
  function setIndeterminate(node: HTMLInputElement, value: boolean) {
    node.indeterminate = value;
    return {
      update(value: boolean) {
        node.indeterminate = value;
      },
    };
  }

  type SelectionKind = "expense" | "income";

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

  // "All Time" fetches a fixed number of transactions per request instead of
  // the whole ledger in one shot — a single query spanning decades was the
  // slow path this pagination replaces. Batching by a row count rather than
  // a calendar year is deliberate: a year of history can be one transaction
  // or a hundred thousand depending on the user, so only a count keeps each
  // batch cheap regardless of how activity is distributed over time.
  const PAGE_SIZE = 300;
  let nextOffset = $state(0);
  let allTimeExhausted = $state(false);
  let loadingMore = $state(false);
  let sentinel = $state<HTMLDivElement | null>(null);

  type SortField = "date" | "amount" | "source" | "category";
  let sortField = $state<SortField>("date");
  let sortDir = $state<"asc" | "desc">("desc");
  let categoryFilter = $state("");
  let sourceFilter = $state("");

  // Expenses and Income keep independent selections — checking a row in one
  // list never surfaces the other list's bulk-action menu. A plain `Set`
  // wrapped in `$state` only reacts to reassignment, not to `.add`/
  // `.delete` on the same instance — `SvelteSet` is the reactive-collection
  // variant that tracks mutation.
  let selectedExpenseIds = new SvelteSet<string>();
  let selectedIncomeIds = new SvelteSet<string>();

  // The row a plain click last landed on, per list — the anchor a
  // following shift-click ranges from. Cleared whenever the selection it
  // refers to no longer means anything (reload, bulk action, a row it
  // pointed at scrolling out of the current filter).
  let lastClickedExpenseId = $state<string | null>(null);
  let lastClickedIncomeId = $state<string | null>(null);

  let formDate = $state(todayIsoDate());
  let formAmount = $state("");
  let formSource = $state("");
  let formCategoryId = $state("");
  let formAccountId = $state("");

  onMount(load);

  async function load() {
    loading = true;
    error = "";
    allTimeExhausted = false;
    // A new range or a reload invalidates whatever was on screen — never
    // leave a stale selection armed against rows that are about to change.
    selectedExpenseIds.clear();
    selectedIncomeIds.clear();
    lastClickedExpenseId = null;
    lastClickedIncomeId = null;
    currentRange = computeRange(rangeMode, {
      start: customStart,
      end: customEnd,
    });
    try {
      const [a, c] = await Promise.all([
        api.listAccounts(),
        api.listCategories(),
        refreshCount(),
      ]);
      accounts = a;
      categories = c;

      if (rangeMode === "all") {
        const batch = await api.listTransactionsPage(0, PAGE_SIZE);
        transactions = batch;
        nextOffset = batch.length;
        allTimeExhausted = batch.length < PAGE_SIZE;
      } else {
        transactions = await api.listTransactions(
          currentRange.start,
          currentRange.end,
        );
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function loadMorePage() {
    if (rangeMode !== "all" || allTimeExhausted || loadingMore) return;
    loadingMore = true;
    try {
      const batch = await api.listTransactionsPage(nextOffset, PAGE_SIZE);
      // The range mode (or the whole page) may have moved on while this was
      // in flight — don't splice a stale batch into whatever's showing now.
      if (rangeMode !== "all") return;
      transactions = [...transactions, ...batch];
      nextOffset += batch.length;
      if (batch.length < PAGE_SIZE) allTimeExhausted = true;
    } catch (e) {
      error = String(e);
    } finally {
      loadingMore = false;
    }
  }

  $effect(() => {
    if (rangeMode !== "all" || !sentinel) return;
    const target = sentinel;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) loadMorePage();
      },
      { rootMargin: "200px" },
    );
    observer.observe(target);
    return () => observer.disconnect();
  });

  let showScrollTop = $state(false);

  $effect(() => {
    function onScroll() {
      showScrollTop = window.scrollY > 400;
    }
    window.addEventListener("scroll", onScroll);
    onScroll();
    return () => window.removeEventListener("scroll", onScroll);
  });

  function scrollToTop() {
    window.scrollTo({ top: 0, behavior: "smooth" });
  }

  // The header count is the true total matching the current range and
  // filters — not `transactions.length`, which for "All Time" is only
  // whatever's been paged in so far.
  let totalCount = $state(0);

  async function refreshCount() {
    try {
      totalCount = await api.countTransactions(
        currentRange.start,
        currentRange.end,
        categoryFilter || null,
        sourceFilter.trim() || null,
      );
    } catch {
      // Header count is supplementary — a failed refresh just leaves the
      // previous total showing rather than surfacing its own error.
    }
  }

  // `load()` calls `refreshCount()` directly (in step with `loading`, so the
  // header never flashes a stale total when the range changes). This effect
  // covers what `load()` doesn't — the category/source filters changing
  // without a reload — debounced so typing in the source box doesn't fire
  // an IPC call per keystroke.
  $effect(() => {
    categoryFilter;
    sourceFilter;
    const timer = setTimeout(refreshCount, 250);
    return () => clearTimeout(timer);
  });

  // A filter that hides a selected row must drop it from the selection, not
  // just visually hide it — otherwise clearing the filter later would
  // silently resurrect an old selection the user never re-confirmed.
  // `untrack` reads each Set's current members without subscribing this
  // effect to the Set itself, so deleting from it here can't re-trigger the
  // same effect.
  function pruneToVisible(selected: Set<string>, visibleIds: Set<string>) {
    for (const id of untrack(() => Array.from(selected))) {
      if (!visibleIds.has(id)) selected.delete(id);
    }
  }

  $effect(() => {
    const visibleIds = new Set(filteredTransactions.map((t) => t.id));
    pruneToVisible(selectedExpenseIds, visibleIds);
    pruneToVisible(selectedIncomeIds, visibleIds);
    // A shift-click anchor pointing at a row that's no longer visible would
    // range-select against a row the user can't see.
    if (lastClickedExpenseId && !visibleIds.has(lastClickedExpenseId)) {
      lastClickedExpenseId = null;
    }
    if (lastClickedIncomeId && !visibleIds.has(lastClickedIncomeId)) {
      lastClickedIncomeId = null;
    }
  });

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
  let categoryFilterOptions = $derived([
    { id: "", label: "All categories" },
    ...categoryOptions,
  ]);

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
      transactions = transactions.map((tx) =>
        tx.id === t.id ? { ...tx, category_id: categoryId } : tx,
      );
      // Recategorizing can move this transaction in or out of an active
      // category filter's count.
      refreshCount();
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function handleDelete(t: TransactionDto) {
    try {
      await api.deleteTransaction(t.id);
      await load();
      // Deleting one leg of a transfer deletes the other, on an account the
      // user may not even be looking at — say so rather than let a balance
      // change somewhere else go unexplained.
      toast.success(
        t.role === "transfer"
          ? "Transfer deleted, on both accounts."
          : "Transaction deleted.",
      );
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

  let filteredTransactions = $derived.by(() => {
    const source = sourceFilter.trim().toLowerCase();
    return transactions.filter(
      (t) =>
        (!categoryFilter || t.category_id === categoryFilter) &&
        (!source || t.source.toLowerCase().includes(source)),
    );
  });
  let expenses = $derived(
    sortTransactions(
      filteredTransactions.filter((t) => t.amount_minor_units < 0),
    ),
  );
  let income = $derived(
    sortTransactions(
      filteredTransactions.filter((t) => t.amount_minor_units > 0),
    ),
  );

  // The selection restricted to rows this list is actually showing right
  // now — after filters, and in "All Time" only the pages loaded so far.
  // Every bulk action operates on this, never on the raw Set, so a select
  // action can never reach a row the user can't see.
  let visibleSelectedExpenseIds = $derived(
    expenses.filter((t) => selectedExpenseIds.has(t.id)).map((t) => t.id),
  );
  let visibleSelectedIncomeIds = $derived(
    income.filter((t) => selectedIncomeIds.has(t.id)).map((t) => t.id),
  );

  function selectionSet(kind: SelectionKind): Set<string> {
    return kind === "expense" ? selectedExpenseIds : selectedIncomeIds;
  }

  function lastClickedId(kind: SelectionKind): string | null {
    return kind === "expense" ? lastClickedExpenseId : lastClickedIncomeId;
  }

  function setLastClickedId(kind: SelectionKind, id: string | null) {
    if (kind === "expense") lastClickedExpenseId = id;
    else lastClickedIncomeId = id;
  }

  function toggleRowSelection(kind: SelectionKind, id: string) {
    const set = selectionSet(kind);
    if (set.has(id)) set.delete(id);
    else set.add(id);
  }

  /** Plain click toggles just this row and becomes the new anchor.
   * Shift-click extends from the last-clicked row (in the sorted order
   * currently on screen) through this one, adding the whole range to the
   * selection — it never deselects, matching the file-manager convention
   * this is modeled on. Falls back to a plain toggle when there's no usable
   * anchor (first click, or the anchor scrolled out of view). */
  function handleRowCheckboxClick(
    kind: SelectionKind,
    id: string,
    event: MouseEvent,
  ) {
    const items = kind === "expense" ? expenses : income;
    const anchor = lastClickedId(kind);
    if (event.shiftKey && anchor && anchor !== id) {
      const anchorIndex = items.findIndex((t) => t.id === anchor);
      const targetIndex = items.findIndex((t) => t.id === id);
      if (anchorIndex !== -1 && targetIndex !== -1) {
        const [start, end] =
          anchorIndex < targetIndex
            ? [anchorIndex, targetIndex]
            : [targetIndex, anchorIndex];
        const set = selectionSet(kind);
        for (let i = start; i <= end; i++) set.add(items[i].id);
        setLastClickedId(kind, id);
        return;
      }
    }
    toggleRowSelection(kind, id);
    setLastClickedId(kind, id);
  }

  // Click-and-drag multi-select: press the mouse down on a checkbox, then
  // drag over other rows to sweep them into (or out of) the selection —
  // same list only, and only while the button stays down.
  let dragKind: SelectionKind | null = $state(null);
  let dragPaintValue = $state(false);
  // Not $state: only ever read from endRowDrag, which itself only runs from
  // a real mouseup — no render depends on its value between those two.
  let dragLastId: string | null = null;

  /** Starts (or, for a shift-click, just performs) the row action on mouse
   * down rather than click — has to happen this early so the drag can pick
   * up the very next row the cursor enters, not just the ones after. */
  function beginRowDrag(kind: SelectionKind, id: string, event: MouseEvent) {
    handleRowCheckboxClick(kind, id, event);
    if (event.shiftKey) return; // a discrete range-select, not a drag
    dragKind = kind;
    dragPaintValue = selectionSet(kind).has(id);
    dragLastId = id;
  }

  /** Sweeps `id` into the drag's outcome — checked if the row the drag
   * started on just became checked, unchecked if it just became unchecked —
   * so dragging back over already-swept rows doesn't flicker them. */
  function continueRowDrag(kind: SelectionKind, id: string) {
    if (dragKind !== kind) return;
    const set = selectionSet(kind);
    if (dragPaintValue) set.add(id);
    else set.delete(id);
    dragLastId = id;
  }

  function endRowDrag() {
    if (dragKind && dragLastId) setLastClickedId(dragKind, dragLastId);
    dragKind = null;
    dragLastId = null;
  }

  function toggleSelectAll(kind: SelectionKind) {
    const items = kind === "expense" ? expenses : income;
    const set = selectionSet(kind);
    const allSelected = items.length > 0 && items.every((t) => set.has(t.id));
    if (allSelected) {
      set.clear();
    } else {
      for (const t of items) set.add(t.id);
    }
    // A range anchored on a row from before the select-all is a stale
    // reference now that the whole list's checked state changed at once.
    setLastClickedId(kind, null);
  }

  async function handleBulkDelete(kind: SelectionKind) {
    const ids =
      kind === "expense" ? visibleSelectedExpenseIds : visibleSelectedIncomeIds;
    if (ids.length === 0) return;
    try {
      const outcome = await api.deleteTransactions(ids);
      // The backend expands each id to its whole transfer group, so a
      // counterpart leg can be removed even though it was never selected —
      // and, since this page mixes every account, it's almost always
      // already loaded here too (in the *other* list). Drop it locally by
      // matching on transfer_group_id rather than trusting `ids` alone.
      const idSet = new Set(ids);
      const groupIds = new Set(
        transactions
          .filter((t) => idSet.has(t.id) && t.transfer_group_id)
          .map((t) => t.transfer_group_id as string),
      );
      transactions = transactions.filter(
        (t) =>
          !idSet.has(t.id) &&
          !(t.transfer_group_id && groupIds.has(t.transfer_group_id)),
      );
      selectionSet(kind).clear();
      setLastClickedId(kind, null);
      await refreshCount();
      toast.success(
        outcome.transfer_groups > 0
          ? `${outcome.deleted} transactions deleted (${outcome.transfer_groups} transfer${outcome.transfer_groups === 1 ? "" : "s"} removed on both accounts).`
          : `${outcome.deleted} transaction${outcome.deleted === 1 ? "" : "s"} deleted.`,
      );
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function handleBulkRecategorize(kind: SelectionKind, categoryId: string) {
    const ids =
      kind === "expense" ? visibleSelectedExpenseIds : visibleSelectedIncomeIds;
    if (ids.length === 0 || !categoryId) return;
    try {
      await api.setTransactionsCategory(ids, categoryId);
      const idSet = new Set(ids);
      transactions = transactions.map((t) =>
        idSet.has(t.id) ? { ...t, category_id: categoryId } : t,
      );
      selectionSet(kind).clear();
      setLastClickedId(kind, null);
      // Only matters when a category filter is active — the prune effect
      // then drops whatever rows no longer match it.
      if (categoryFilter) await refreshCount();
    } catch (e) {
      toast.error(String(e));
    }
  }
</script>

<!-- Catches the drag's mouseup wherever it lands, including outside any
     row — the button can be released past the last row, past the edge of
     the table, anywhere. -->
<svelte:window onmouseup={endRowDrag} />

{#snippet checkbox(props: {
  checked: boolean;
  indeterminate?: boolean;
  ariaLabel: string;
  onpress: (event: MouseEvent) => void;
})}
  <label class="checkbox" class:checked={props.checked} class:indeterminate={props.indeterminate}>
    <input
      type="checkbox"
      checked={props.checked}
      use:setIndeterminate={!!props.indeterminate}
      aria-label={props.ariaLabel}
      onmousedown={(event) => {
        // Fires on mousedown, not click, for two reasons: a browser starts
        // extending its own text selection right here, before any click
        // handler would even run, so preventing default is what stops a
        // shift-click (or a drag) from also sweeping up the row text as a
        // selection — and a following drag needs the state already applied
        // by the time the cursor reaches the next row, not a beat later.
        event.preventDefault();
        props.onpress(event);
      }}
      onclick={(event) => {
        // A checkbox's own checked-state flip is tied to the `click` event
        // specifically (browsers pre-toggle it before dispatch, then revert
        // if the click is prevented) — preventing mousedown's default above
        // doesn't touch that. Without this, the native toggle fires right
        // alongside our own, fighting `checked={props.checked}` for which
        // one wins. All the actual logic already ran on mousedown; this is
        // just here to keep the native behavior out of the way.
        event.preventDefault();
      }}
    />
    <span class="box">
      {#if props.indeterminate}
        <Minus size={13} strokeWidth={3} />
      {:else if props.checked}
        <Check size={13} strokeWidth={3} />
      {/if}
    </span>
  </label>
{/snippet}

{#snippet list(items: TransactionDto[], kind: SelectionKind)}
    {@const selected = selectionSet(kind)}
    {@const anySelected = items.some((t) => selected.has(t.id))}
    {@const allSelected = items.length > 0 && items.every((t) => selected.has(t.id))}
    <table>
      <thead>
        <tr>
          <th class="select-header">
            {@render checkbox({
              checked: allSelected,
              indeterminate: anySelected && !allSelected,
              ariaLabel: `Select all ${kind === "expense" ? "expenses" : "income"}`,
              onpress: () => toggleSelectAll(kind),
            })}
          </th>
          <th class="date-cell"
            ><button type="button" onclick={() => toggleSort("date")}
              >Date</button
            ></th
          >
          <th
            ><button type="button" onclick={() => toggleSort("amount")}
              >Amount</button
            ></th
          >
          <th>
            <div class="column-header">
              <button type="button" onclick={() => toggleSort("source")}
                >Source</button
              >
              <FilterPopover
                active={sourceFilter.trim() !== ""}
                ariaLabel="Filter by source"
              >
                <input
                  bind:value={sourceFilter}
                  use:autofocus
                  placeholder="Search source…"
                  spellcheck="false"
                  autocomplete="off"
                  autocorrect="off"
                  autocapitalize="off"
                />
              </FilterPopover>
            </div>
          </th>
          <th>
            <div class="column-header">
              <button type="button" onclick={() => toggleSort("category")}
                >Category</button
              >
              <CategorySelect
                options={categoryFilterOptions}
                value={categoryFilter}
                onChange={(id) => (categoryFilter = id)}
              >
                {#snippet trigger()}
                  <Search size={14} aria-label="Filter by category" />
                {/snippet}
              </CategorySelect>
            </div>
          </th>
          <th>Account</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#if items.length === 0}
          <tr><td class="empty" colspan="7">No transactions.</td></tr>
        {:else}
          {#each items as t (t.id)}
            <tr onmouseenter={() => continueRowDrag(kind, t.id)}>
              <td class="select-cell">
                {@render checkbox({
                  checked: selected.has(t.id),
                  ariaLabel: `Select transaction ${t.date} ${t.source}`,
                  onpress: (event: MouseEvent) => beginRowDrag(kind, t.id, event),
                })}
              </td>
              <td class="date-cell">{t.date}</td>
              <td>{formatCurrency(t.amount_minor_units, t.currency)}</td>
              <td>
                {t.source}
                {#if t.role === "transfer"}
                  <span class="role-badge" title="Between your own accounts — not counted as spending"
                    >transfer</span
                  >
                {:else if t.role === "adjustment"}
                  <span class="role-badge" title="Reconciliation — not counted as spending"
                    >adjustment</span
                  >
                {/if}
              </td>
              <td>
                <CategorySelect
                  options={categoryOptions}
                  value={t.category_id}
                  onChange={(categoryId) => handleCategoryChange(t, categoryId)}
                />
              </td>
              <td>{accountName(t.account_id)}</td>
              <td>
                <DeleteButton
                  label="Delete transaction"
                  onConfirm={() => handleDelete(t)}
                />
              </td>
            </tr>
          {/each}
        {/if}
      </tbody>
    </table>
{/snippet}

<div class="title">
  <h1>Transactions</h1>
  {#if !loading}
    <span class="summary">{totalCount} transactions</span>
  {/if}
</div>

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
      <div class="section-header">
        <h2>Expenses</h2>
        {#if visibleSelectedExpenseIds.length > 0}
          <div
            class="bulk-actions"
            role="toolbar"
            aria-label="Bulk actions for expenses"
            aria-live="polite"
          >
            <span class="bulk-count">{visibleSelectedExpenseIds.length} selected</span>
            <CategorySelect
              options={categoryOptions}
              value=""
              onChange={(id) => handleBulkRecategorize("expense", id)}
            >
              {#snippet trigger()}
                <Pencil size={14} aria-label="Recategorize selected expenses" />
              {/snippet}
            </CategorySelect>
            <DeleteButton
              compact
              label={`Delete ${visibleSelectedExpenseIds.length} transactions`}
              onConfirm={() => handleBulkDelete("expense")}
            />
          </div>
        {/if}
      </div>
      {@render list(expenses, "expense")}
    </section>
    <section>
      <div class="section-header">
        <h2>Income</h2>
        {#if visibleSelectedIncomeIds.length > 0}
          <div
            class="bulk-actions"
            role="toolbar"
            aria-label="Bulk actions for income"
            aria-live="polite"
          >
            <span class="bulk-count">{visibleSelectedIncomeIds.length} selected</span>
            <CategorySelect
              options={categoryOptions}
              value=""
              onChange={(id) => handleBulkRecategorize("income", id)}
            >
              {#snippet trigger()}
                <Pencil size={14} aria-label="Recategorize selected income" />
              {/snippet}
            </CategorySelect>
            <DeleteButton
              compact
              label={`Delete ${visibleSelectedIncomeIds.length} transactions`}
              onConfirm={() => handleBulkDelete("income")}
            />
          </div>
        {/if}
      </div>
      {@render list(income, "income")}
    </section>
  </div>
  {#if rangeMode === "all"}
    <div bind:this={sentinel} class="scroll-sentinel">
      {#if loadingMore}
        <p class="scroll-status">Loading more…</p>
      {:else if allTimeExhausted}
        <p class="scroll-status">All transactions loaded.</p>
      {/if}
    </div>
  {/if}
{/if}

{#if showScrollTop}
  <button
    type="button"
    class="icon-button scroll-top-button"
    aria-label="Scroll to top"
    title="Scroll to top"
    onclick={scrollToTop}
  >
    <ArrowUp size={18} />
  </button>
{/if}

<style>
  h1 {
    margin-top: 0;
  }

  .title {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
  }

  .summary {
    font-size: 0.85rem;
    opacity: 0.6;
    white-space: nowrap;
  }

  .error {
    color: var(--color-danger);
  }

  .empty {
    opacity: 0.75;
    padding: 0.6rem 0.5rem;
  }

  /* Marks a row that is deliberately absent from Overview and Details, so
     the ledger and the reports can't look like they disagree. */
  .role-badge {
    margin-left: 0.4rem;
    padding: 0.05rem 0.35rem;
    border: 1px solid var(--color-border);
    border-radius: 0.5rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    opacity: 0.7;
    white-space: nowrap;
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
    /* Breathing room above "Expenses"/"Income", separate from the range
       bar's own margin-bottom. */
    margin-top: 1.5rem;
  }

  /* Thin cyan rule between the two lists — grid items stretch to the row's
     full height by default, so this runs top to bottom without extra work. */
  .lists > section:last-child {
    border-left: .5px solid var(--color-accent);
    padding-left: 1rem;
  }

  .scroll-sentinel {
    min-height: 1px;
  }

  .scroll-status {
    text-align: center;
    opacity: 0.6;
    font-size: 0.85rem;
    padding: 1rem 0;
  }

  .scroll-top-button {
    position: fixed;
    bottom: 1.5rem;
    right: 1.5rem;
    width: 2.5rem;
    height: 2.5rem;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
    z-index: 10;
  }

  h2 {
    font-size: 1.1rem;
    margin: 0;
  }

  .section-header {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.75rem;
    margin-bottom: 0.6rem;
    /* Reserves the height the bulk-actions pill needs so the table below
       doesn't shift down the moment a selection starts. */
    min-height: 2rem;
    /* Lines the heading up with "Date", not with the checkbox column to its
       left: 2rem for .select-header's width plus the 0.5rem left padding
       every th/td gets, so "Date" text and "Expenses"/"Income" text share
       the same left edge. */
    padding-left: 2.5rem;
  }

  .bulk-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    background-color: var(--color-shade-2);
    border-radius: 999px;
    padding: 0.2rem 0.5rem 0.2rem 0.8rem;
  }

  .bulk-count {
    font-size: 0.8rem;
    opacity: 0.75;
    white-space: nowrap;
  }

  .select-header,
  .select-cell {
    width: 2rem;
    padding-right: 0;
    /* The row-separator line starts at the Date column, not out here. */
    border-bottom: none;
  }

  /* Custom checkbox: a real (visually hidden) input for behavior and a11y,
     with a styled box drawn next to it — native checkboxes can't have their
     checkmark recolored independently of the box fill, and this one needs
     to read as cyan-box / background-colored glyph, like every other icon
     control in the app (see .icon-button in app.css). */
  .checkbox {
    display: inline-flex;
    position: relative;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.1s;
  }

  .checkbox input {
    /* Without this, some engines keep hit-testing against the native
       checkbox widget's own default-sized hotspot instead of the CSS box
       `inset: 0` stretches it to — the visible part is all drawn by `.box`
       below anyway, so the input has no native look left to preserve. */
    appearance: none;
    -webkit-appearance: none;
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }

  .box {
    width: 1.35rem;
    height: 1.35rem;
    border-radius: 5px;
    border: 1.5px solid var(--color-shade-4);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent-contrast);
    pointer-events: none;
  }

  .checkbox.checked .box,
  .checkbox.indeterminate .box {
    background-color: var(--color-accent);
    border-color: var(--color-accent);
  }

  /* Hidden until its row/header is hovered, focused, or already checked —
     a bare checkbox column would otherwise clutter every row for a feature
     most visits never use. */
  tr:hover .select-cell .checkbox,
  thead tr:hover .select-header .checkbox,
  .checkbox.checked,
  .checkbox.indeterminate,
  .checkbox:focus-within {
    opacity: 1;
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

  .column-header {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  td {
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid var(--color-shade-2);
  }

  /* Targeted by class, not :first-child — the checkbox column sits to its
     left now, and a position-based selector silently stops matching the
     Date column the moment the column order changes again (already bit us
     once when this column added the leading checkbox). */
  .date-cell {
    white-space: nowrap;
  }

  @media (max-width: 900px) {
    .lists {
      grid-template-columns: 1fr;
    }
  }
</style>
