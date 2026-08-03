<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
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
    oneMonthAgoIsoDate,
    operationKindLabel,
    type AccountDto,
    type CategoryDto,
    type TransactionDto,
    type RangeMode,
  } from "$lib/api";
  import ImportCsvDialog from "$lib/ImportCsvDialog.svelte";
  import Checkbox from "$lib/Checkbox.svelte";
  import DeleteButton from "$lib/DeleteButton.svelte";
  import CategorySelect from "$lib/CategorySelect.svelte";
  import FilterPopover from "$lib/FilterPopover.svelte";
  import DateRangePicker from "$lib/DateRangePicker.svelte";
  import { pageViewState } from "$lib/pageCache";
  import { toast } from "$lib/toasts.svelte";
  import { ArrowUp, FileUp, Pencil, Plus, Search } from "@lucide/svelte";

  function autofocus(node: HTMLElement) {
    node.focus();
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

  /** An import is the moment an account's balance silently goes wrong: it
   * now has history, but nothing says what the account held before that
   * history starts. Named here rather than left for the user to notice on
   * the Accounts page, because the number looks perfectly plausible. One
   * import can land rows on several accounts via transfer rules, so this
   * checks all of them rather than just the chosen destination. */
  async function handleImported() {
    await load();
    const unanchored = accounts.filter(
      (a) => !a.is_opening_balance_set && a.has_transactions,
    );
    if (unanchored.length === 0) return;
    const names = unanchored.map((a) => `"${a.name}"`).join(", ");
    toast.error(
      `Balances for ${names} are off until you set a starting point — do it in Accounts.`,
    );
  }

  let accounts = $state<AccountDto[]>([]);
  let categories = $state<CategoryDto[]>([]);
  // Holds the ledger rows for Month/Year/Custom ranges — unfiltered and
  // mixed-sign, since those ranges fetch everything in one shot and split
  // it client-side. "All Time" instead pages `expenseRows`/`incomeRows`
  // directly, since the backend applies each list's own filters and sign.
  let transactions = $state<TransactionDto[]>([]);
  let expenseRows = $state<TransactionDto[]>([]);
  let incomeRows = $state<TransactionDto[]>([]);
  let loading = $state(true);
  let error = $state("");

  // Everything the user set by hand — range, sort, filters — plus where they
  // had scrolled to and how much of the ledger that position was measured
  // against, kept across navigation. See `$lib/pageCache`: view state only,
  // the rows themselves are always re-fetched on mount.
  const view = pageViewState("transactions", () => ({
    rangeMode: "month" as RangeMode,
    customStart: oneMonthAgoIsoDate(),
    customEnd: todayIsoDate(),
    sortField: "date" as SortField,
    sortDir: "desc" as "asc" | "desc",
    // Expenses and Income keep independent category/description filters — a
    // search on one list must never narrow the other.
    expenseCategoryFilter: "",
    expenseDescriptionFilter: "",
    incomeCategoryFilter: "",
    incomeDescriptionFilter: "",
    loadedExpenseRows: 0,
    loadedIncomeRows: 0,
    scrollY: 0,
  }));

  let rangeMode = $state<RangeMode>(view.rangeMode);
  let customStart = $state(view.customStart);
  let customEnd = $state(view.customEnd);
  let currentRange = $state({ start: todayIsoDate(), end: todayIsoDate() });

  // "All Time" fetches a fixed number of transactions per request instead of
  // the whole ledger in one shot — a single query spanning decades was the
  // slow path this pagination replaces. Batching by a row count rather than
  // a calendar year is deliberate: a year of history can be one transaction
  // or a hundred thousand depending on the user, so only a count keeps each
  // batch cheap regardless of how activity is distributed over time.
  //
  // Expenses and Income page through the ledger independently — each has
  // its own filters, so a single shared cursor/exhausted flag would mean
  // one list's filter change could stall the other's pagination.
  const PAGE_SIZE = 300;
  let expenseOffset = $state(0);
  let incomeOffset = $state(0);
  let expenseExhausted = $state(false);
  let incomeExhausted = $state(false);
  let loadingMoreExpense = $state(false);
  let loadingMoreIncome = $state(false);
  let loadingMore = $derived(loadingMoreExpense || loadingMoreIncome);
  let allTimeExhausted = $derived(expenseExhausted && incomeExhausted);
  let sentinel = $state<HTMLDivElement | null>(null);

  // Bumped by anything that invalidates the rows on screen for that list —
  // a range change, a reload, a filter change. A fetch that comes back
  // holding a stale token is answering a question the user has already
  // moved on from, so its batch is dropped instead of spliced in. Separate
  // per kind so a filter change on one list can't cancel a fetch already in
  // flight for the other.
  let expenseToken = 0;
  let incomeToken = 0;
  // A full `load()` (range change, mount) invalidates whatever either list
  // had in flight — bumping this alongside the per-kind tokens is how a
  // reload started mid-fetch wins over a batch that was already on the way.
  let loadToken = 0;

  function bumpToken(kind: SelectionKind): number {
    if (kind === "expense") return ++expenseToken;
    return ++incomeToken;
  }

  function currentToken(kind: SelectionKind): number {
    return kind === "expense" ? expenseToken : incomeToken;
  }

  type SortField = "date" | "amount" | "description" | "category";
  let sortField = $state<SortField>(view.sortField);
  let sortDir = $state<"asc" | "desc">(view.sortDir);
  let expenseCategoryFilter = $state(view.expenseCategoryFilter);
  let expenseDescriptionFilter = $state(view.expenseDescriptionFilter);
  let incomeCategoryFilter = $state(view.incomeCategoryFilter);
  let incomeDescriptionFilter = $state(view.incomeDescriptionFilter);

  // Mirrors the user's choices back into the cache. `loadedExpenseRows` /
  // `loadedIncomeRows` only mean anything in "All Time" — every other range
  // fetches its rows in one shot, so there's no page count to restore.
  $effect(() => {
    view.rangeMode = rangeMode;
    view.customStart = customStart;
    view.customEnd = customEnd;
    view.sortField = sortField;
    view.sortDir = sortDir;
    view.expenseCategoryFilter = expenseCategoryFilter;
    view.expenseDescriptionFilter = expenseDescriptionFilter;
    view.incomeCategoryFilter = incomeCategoryFilter;
    view.incomeDescriptionFilter = incomeDescriptionFilter;
    view.loadedExpenseRows = rangeMode === "all" ? expenseOffset : 0;
    view.loadedIncomeRows = rangeMode === "all" ? incomeOffset : 0;
  });

  /** Where the user was reading when they left, if anywhere — read once here
   * at init, before the effects above start overwriting the cache with this
   * mount's own (still empty) state. Consumed by the first `load()`: every
   * later one is the user asking for a different set of rows, and jumping
   * them back down the page then would be wrong. */
  let pendingRestore: {
    expenseRows: number;
    incomeRows: number;
    scrollY: number;
  } | null =
    view.scrollY > 0
      ? {
          expenseRows: view.loadedExpenseRows,
          incomeRows: view.loadedIncomeRows,
          scrollY: view.scrollY,
        }
      : null;

  /** Puts the viewport back where it was, once the rows it was measured
   * against are on screen — `tick()` waits for `loading = false` to have
   * actually rendered the table, since scrolling a page that is still one
   * "Loading…" line tall goes nowhere. */
  async function restoreScrollPosition() {
    const target = pendingRestore;
    pendingRestore = null;
    if (!target) return;
    await tick();
    window.scrollTo(0, target.scrollY);
  }

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
  let formDescription = $state("");
  let formCategoryId = $state("");
  let formAccountId = $state("");

  onMount(load);

  /** A list's filters in the shape the backend takes them, empty meaning
   * "no filter", not "match the empty string". */
  function activeFilters(kind: SelectionKind): {
    category: string | null;
    description: string | null;
  } {
    return kind === "expense"
      ? {
          category: expenseCategoryFilter || null,
          description: expenseDescriptionFilter.trim() || null,
        }
      : {
          category: incomeCategoryFilter || null,
          description: incomeDescriptionFilter.trim() || null,
        };
  }

  /** Identity of the filter pair a list's rows on screen were fetched with,
   * so the debounced effect below can tell a real filter change from its
   * own first run after `load()` already fetched with these same values. */
  function filterKey(kind: SelectionKind): string {
    const { category, description } = activeFilters(kind);
    return `${category ?? ""} ${description ?? ""}`;
  }

  let appliedExpenseFilterKey = "";
  let appliedIncomeFilterKey = "";

  async function load() {
    loading = true;
    error = "";
    expenseExhausted = false;
    incomeExhausted = false;
    const token = ++loadToken;
    bumpToken("expense");
    bumpToken("income");
    appliedExpenseFilterKey = filterKey("expense");
    appliedIncomeFilterKey = filterKey("income");
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
        refreshCount("expense"),
        refreshCount("income"),
      ]);
      accounts = a;
      categories = c;

      if (rangeMode === "all") {
        const expenseFilters = activeFilters("expense");
        const incomeFilters = activeFilters("income");
        // Coming back to a list the user had already scrolled a long way
        // down has to bring back every page they'd scrolled in, not just the
        // first — otherwise there is no list left underneath the position
        // we're restoring to. One wider query rather than replaying the
        // batches one by one: the scroll position is only right once all of
        // them are on screen anyway.
        const expenseLimit = Math.max(PAGE_SIZE, pendingRestore?.expenseRows ?? 0);
        const incomeLimit = Math.max(PAGE_SIZE, pendingRestore?.incomeRows ?? 0);
        const [expenseBatch, incomeBatch] = await Promise.all([
          api.listTransactionsPage(
            0,
            expenseLimit,
            expenseFilters.category,
            expenseFilters.description,
            false,
          ),
          api.listTransactionsPage(
            0,
            incomeLimit,
            incomeFilters.category,
            incomeFilters.description,
            true,
          ),
        ]);
        if (token !== loadToken) return;
        expenseRows = expenseBatch;
        incomeRows = incomeBatch;
        expenseOffset = expenseBatch.length;
        incomeOffset = incomeBatch.length;
        expenseExhausted = expenseBatch.length < expenseLimit;
        incomeExhausted = incomeBatch.length < incomeLimit;
      } else {
        const rows = await api.listTransactions(
          currentRange.start,
          currentRange.end,
        );
        if (token !== loadToken) return;
        transactions = rows;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
    await restoreScrollPosition();
    fillViewport();
  }

  /** Restarts a list's "All Time" pagination from offset 0 with its current
   * filters. A filter change there is a different query, not a narrowing of
   * what's already on screen: the matching rows can live anywhere in the
   * ledger, including pages that were never fetched — which is why
   * filtering only the batches loaded so far made a filter look like it had
   * found almost nothing until the whole ledger had been scrolled in.
   * Deliberately doesn't touch `loading`: that would unmount the table, and
   * with it the filter control the user is still interacting with. */
  async function reloadFilteredAllTimeKind(kind: SelectionKind) {
    const token = bumpToken(kind);
    const { category, description } = activeFilters(kind);
    if (kind === "expense") loadingMoreExpense = true;
    else loadingMoreIncome = true;
    try {
      const batch = await api.listTransactionsPage(
        0,
        PAGE_SIZE,
        category,
        description,
        kind === "income",
      );
      if (token !== currentToken(kind) || rangeMode !== "all") return;
      if (kind === "expense") {
        expenseRows = batch;
        expenseOffset = batch.length;
        expenseExhausted = batch.length < PAGE_SIZE;
      } else {
        incomeRows = batch;
        incomeOffset = batch.length;
        incomeExhausted = batch.length < PAGE_SIZE;
      }
    } catch (e) {
      error = String(e);
    } finally {
      if (token === currentToken(kind)) {
        if (kind === "expense") loadingMoreExpense = false;
        else loadingMoreIncome = false;
      }
    }
    if (token === currentToken(kind)) await fillViewportKind(kind);
  }

  async function loadMoreKind(kind: SelectionKind) {
    const exhausted = kind === "expense" ? expenseExhausted : incomeExhausted;
    const alreadyLoading =
      kind === "expense" ? loadingMoreExpense : loadingMoreIncome;
    if (rangeMode !== "all" || exhausted || alreadyLoading) return;
    const token = currentToken(kind);
    if (kind === "expense") loadingMoreExpense = true;
    else loadingMoreIncome = true;
    try {
      const { category, description } = activeFilters(kind);
      const offset = kind === "expense" ? expenseOffset : incomeOffset;
      const batch = await api.listTransactionsPage(
        offset,
        PAGE_SIZE,
        category,
        description,
        kind === "income",
      );
      // The range mode, this list's filters, or the whole page may have
      // moved on while this was in flight — don't splice a stale batch into
      // whatever's showing now.
      if (token !== currentToken(kind) || rangeMode !== "all") return;
      if (kind === "expense") {
        expenseRows = [...expenseRows, ...batch];
        expenseOffset += batch.length;
        if (batch.length < PAGE_SIZE) expenseExhausted = true;
      } else {
        incomeRows = [...incomeRows, ...batch];
        incomeOffset += batch.length;
        if (batch.length < PAGE_SIZE) incomeExhausted = true;
      }
    } catch (e) {
      error = String(e);
    } finally {
      if (kind === "expense") loadingMoreExpense = false;
      else loadingMoreIncome = false;
    }
  }

  function sentinelInView(): boolean {
    if (!sentinel) return false;
    // Same 200px cushion the observer below uses, so both agree on when
    // the bottom of the list counts as "reached".
    return sentinel.getBoundingClientRect().top < window.innerHeight + 200;
  }

  /** An IntersectionObserver only fires when the intersection *changes*, so
   * a sentinel that is still on screen once a batch has landed never
   * re-triggers by itself — the list would sit there half-filled until the
   * user scrolled it again. Keeps pulling pages for a single list until the
   * sentinel is pushed past the viewport or that list's ledger runs out. */
  async function fillViewportKind(kind: SelectionKind) {
    while (
      rangeMode === "all" &&
      !(kind === "expense" ? expenseExhausted : incomeExhausted)
    ) {
      await tick();
      if (!sentinelInView()) return;
      const before = kind === "expense" ? expenseOffset : incomeOffset;
      await loadMoreKind(kind);
      // No progress (a request already in flight, or an error) — stop
      // rather than spin on a condition this loop can't change.
      const after = kind === "expense" ? expenseOffset : incomeOffset;
      if (after === before) return;
    }
  }

  /** Both lists share one scroll sentinel below the two columns, so
   * reaching it keeps both lists' pagination going in step. */
  async function fillViewport() {
    await Promise.all([fillViewportKind("expense"), fillViewportKind("income")]);
  }

  $effect(() => {
    if (rangeMode !== "all" || !sentinel) return;
    const target = sentinel;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) fillViewport();
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
      // Recorded here rather than on unmount: by the time this page is being
      // torn down the browser has often already scrolled the (now much
      // shorter) new page back to the top, so there'd be nothing left to read.
      view.scrollY = window.scrollY;
    }
    window.addEventListener("scroll", onScroll, { passive: true });
    onScroll();
    return () => window.removeEventListener("scroll", onScroll);
  });

  function scrollToTop() {
    window.scrollTo({ top: 0, behavior: "smooth" });
  }

  // The header count is the true total matching the current range and each
  // list's own filters — not `expenses.length + income.length`, which for
  // "All Time" is only whatever's been paged in so far.
  let expenseCount = $state(0);
  let incomeCount = $state(0);
  let totalCount = $derived(expenseCount + incomeCount);

  async function refreshCount(kind: SelectionKind) {
    try {
      const { category, description } = activeFilters(kind);
      const count = await api.countTransactions(
        currentRange.start,
        currentRange.end,
        category,
        description,
        kind === "income",
      );
      if (kind === "expense") expenseCount = count;
      else incomeCount = count;
    } catch {
      // Header count is supplementary — a failed refresh just leaves the
      // previous total showing rather than surfacing its own error.
    }
  }

  // `load()` handles the range changing (fetching in step with `loading`, so
  // the header never flashes a stale total). This effect covers what
  // `load()` doesn't — the category/description filters changing without a
  // reload — debounced so typing in the description box doesn't fire an IPC call
  // per keystroke. In "All Time" the rows themselves have to be re-fetched
  // too, because only part of the ledger is loaded and the matches may be
  // anywhere in the rest of it.
  $effect(() => {
    const expenseKey = filterKey("expense");
    const incomeKey = filterKey("income");
    const timer = setTimeout(() => {
      // `load()` records the key it fetched with, so this skips its own
      // first run on mount (and any range change) rather than re-issuing
      // the identical queries. Each list's filter is checked and refetched
      // independently, so changing one never re-queries the other.
      const expenseChanged = expenseKey !== appliedExpenseFilterKey;
      const incomeChanged = incomeKey !== appliedIncomeFilterKey;
      if (!expenseChanged && !incomeChanged) return;
      if (expenseChanged) {
        appliedExpenseFilterKey = expenseKey;
        refreshCount("expense");
        if (rangeMode === "all") reloadFilteredAllTimeKind("expense");
      }
      if (incomeChanged) {
        appliedIncomeFilterKey = incomeKey;
        refreshCount("income");
        if (rangeMode === "all") reloadFilteredAllTimeKind("income");
      }
    }, 250);
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
    const visibleExpenseIds = new Set(expenses.map((t) => t.id));
    const visibleIncomeIds = new Set(income.map((t) => t.id));
    pruneToVisible(selectedExpenseIds, visibleExpenseIds);
    pruneToVisible(selectedIncomeIds, visibleIncomeIds);
    // A shift-click anchor pointing at a row that's no longer visible would
    // range-select against a row the user can't see.
    if (lastClickedExpenseId && !visibleExpenseIds.has(lastClickedExpenseId)) {
      lastClickedExpenseId = null;
    }
    if (lastClickedIncomeId && !visibleIncomeIds.has(lastClickedIncomeId)) {
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

  function categoryFilterFor(kind: SelectionKind): string {
    return kind === "expense" ? expenseCategoryFilter : incomeCategoryFilter;
  }

  function setCategoryFilter(kind: SelectionKind, id: string) {
    if (kind === "expense") expenseCategoryFilter = id;
    else incomeCategoryFilter = id;
  }

  function descriptionFilterFor(kind: SelectionKind): string {
    return kind === "expense" ? expenseDescriptionFilter : incomeDescriptionFilter;
  }

  function setDescriptionFilter(kind: SelectionKind, value: string) {
    if (kind === "expense") expenseDescriptionFilter = value;
    else incomeDescriptionFilter = value;
  }

  async function handleDescriptionBlur() {
    const description = formDescription.trim();
    if (!description) return;
    if (!formAccountId) {
      try {
        const suggested = await api.suggestAccountForDescription(description);
        if (suggested) formAccountId = suggested;
      } catch {
        // best-effort suggestion only
      }
    }
    if (!formCategoryId) {
      try {
        const suggested = await api.suggestCategoryForDescription(description);
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
        formDescription.trim(),
        formCategoryId,
        formAccountId,
      );
      formAmount = "";
      formDescription = "";
      await load();
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function handleCategoryChange(t: TransactionDto, categoryId: string) {
    if (categoryId === t.category_id) return;
    try {
      await api.setTransactionCategory(t.id, categoryId);
      const update = (tx: TransactionDto) =>
        tx.id === t.id ? { ...tx, category_id: categoryId } : tx;
      transactions = transactions.map(update);
      expenseRows = expenseRows.map(update);
      incomeRows = incomeRows.map(update);
      // Recategorizing can move this transaction in or out of an active
      // category filter's count — only ever the count for its own sign.
      refreshCount(t.amount_minor_units < 0 ? "expense" : "income");
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
      else if (sortField === "description") cmp = a.description.localeCompare(b.description);
      else
        cmp = categoryName(a.category_id).localeCompare(
          categoryName(b.category_id),
        );
      return sortDir === "asc" ? cmp : -cmp;
    });
  }

  // "All Time" already fetches each list pre-filtered and sign-narrowed
  // from the backend — `expenseRows`/`incomeRows` need only sorting. Every
  // other range fetches the whole (unfiltered, mixed-sign) range in one
  // shot, so this list applies its own filter and sign split client-side.
  let expenses = $derived.by(() => {
    if (rangeMode === "all") return sortTransactions(expenseRows);
    const description = expenseDescriptionFilter.trim().toLowerCase();
    return sortTransactions(
      transactions.filter(
        (t) =>
          t.amount_minor_units < 0 &&
          (!expenseCategoryFilter || t.category_id === expenseCategoryFilter) &&
          (!description || t.description.toLowerCase().includes(description)),
      ),
    );
  });
  let income = $derived.by(() => {
    if (rangeMode === "all") return sortTransactions(incomeRows);
    const description = incomeDescriptionFilter.trim().toLowerCase();
    return sortTransactions(
      transactions.filter(
        (t) =>
          t.amount_minor_units > 0 &&
          (!incomeCategoryFilter || t.category_id === incomeCategoryFilter) &&
          (!description || t.description.toLowerCase().includes(description)),
      ),
    );
  });

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
      // and, since a transfer's two legs have opposite signs, it's almost
      // always already loaded here too (in the *other* list). Drop it from
      // every array it might be sitting in by matching on
      // transfer_group_id rather than trusting `ids` alone.
      const idSet = new Set(ids);
      const groupIds = new Set(
        [...transactions, ...expenseRows, ...incomeRows]
          .filter((t) => idSet.has(t.id) && t.transfer_group_id)
          .map((t) => t.transfer_group_id as string),
      );
      const survives = (t: TransactionDto) =>
        !idSet.has(t.id) &&
        !(t.transfer_group_id && groupIds.has(t.transfer_group_id));
      transactions = transactions.filter(survives);
      expenseRows = expenseRows.filter(survives);
      incomeRows = incomeRows.filter(survives);
      selectionSet(kind).clear();
      setLastClickedId(kind, null);
      // A transfer's counterpart can land in either list's count, so both
      // are refreshed regardless of which list the selection was made in.
      await Promise.all([refreshCount("expense"), refreshCount("income")]);
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
      const update = (t: TransactionDto) =>
        idSet.has(t.id) ? { ...t, category_id: categoryId } : t;
      transactions = transactions.map(update);
      expenseRows = expenseRows.map(update);
      incomeRows = incomeRows.map(update);
      selectionSet(kind).clear();
      setLastClickedId(kind, null);
      // Only matters when this list's category filter is active — the
      // prune effect then drops whatever rows no longer match it.
      const categoryFilter =
        kind === "expense" ? expenseCategoryFilter : incomeCategoryFilter;
      if (categoryFilter) await refreshCount(kind);
    } catch (e) {
      toast.error(String(e));
    }
  }
</script>

<!-- Catches the drag's mouseup wherever it lands, including outside any
     row — the button can be released past the last row, past the edge of
     the table, anywhere. -->
<svelte:window onmouseup={endRowDrag} />

{#snippet list(items: TransactionDto[], kind: SelectionKind)}
    {@const selected = selectionSet(kind)}
    {@const anySelected = items.some((t) => selected.has(t.id))}
    {@const allSelected = items.length > 0 && items.every((t) => selected.has(t.id))}
    <table>
      <thead>
        <tr>
          <th class="select-header">
            <Checkbox
              checked={allSelected}
              indeterminate={anySelected && !allSelected}
              ariaLabel={`Select all ${kind === "expense" ? "expenses" : "income"}`}
              onpress={() => toggleSelectAll(kind)}
            />
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
              <button type="button" onclick={() => toggleSort("description")}
                >Description</button
              >
              <FilterPopover
                active={descriptionFilterFor(kind).trim() !== ""}
                ariaLabel="Filter by description"
              >
                <input
                  value={descriptionFilterFor(kind)}
                  oninput={(e) =>
                    setDescriptionFilter(kind, (e.currentTarget as HTMLInputElement).value)}
                  use:autofocus
                  placeholder="Search description…"
                  spellcheck="false"
                  autocomplete="off"
                  autocorrect="off"
                  autocapitalize="off"
                />
              </FilterPopover>
            </div>
          </th>
          <th class="kind-cell">Type</th>
          <th>
            <div class="column-header">
              <button type="button" onclick={() => toggleSort("category")}
                >Category</button
              >
              <CategorySelect
                options={categoryFilterOptions}
                value={categoryFilterFor(kind)}
                onChange={(id) => setCategoryFilter(kind, id)}
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
          <tr><td class="empty" colspan="8">No transactions.</td></tr>
        {:else}
          {#each items as t (t.id)}
            <tr onmouseenter={() => continueRowDrag(kind, t.id)}>
              <td class="select-cell">
                <Checkbox
                  checked={selected.has(t.id)}
                  ariaLabel={`Select transaction ${t.date} ${t.description}`}
                  onpress={(event: MouseEvent) => beginRowDrag(kind, t.id, event)}
                />
              </td>
              <td class="date-cell">{t.date}</td>
              <td>{formatCurrency(t.amount_minor_units, t.currency)}</td>
              <td>
                {t.description}
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
              <td class="kind-cell">{operationKindLabel(t.operation_kind)}</td>
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
    <DateRangePicker
      start={customStart}
      end={customEnd}
      onChange={(s, e) => {
        customStart = s;
        customEnd = e;
        load();
      }}
    />
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
    onImported={handleImported}
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
      placeholder="Description"
      bind:value={formDescription}
      onblur={handleDescriptionBlur}
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
  /* Hidden until its row/header is hovered — a bare checkbox column would
     otherwise clutter every row for a feature most visits never use. A box
     that is checked or focused overrides this from inside Checkbox.svelte,
     so nothing can end up selected but invisible. Driven by a custom
     property because scoped styles here can't reach into that component. */
  .select-cell,
  .select-header {
    --checkbox-opacity: 0;
  }

  tr:hover .select-cell,
  thead tr:hover .select-header {
    --checkbox-opacity: 1;
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

  /* How the money moved. Reads exactly like Account — same colour, size and
     weight — since the two are the same kind of thing: flat context about
     the row, not something ranked above it. Only the width rule is its own:
     `1%` collapses the column to its content so the space goes to
     Description and Category instead. */
  .kind-cell {
    white-space: nowrap;
    width: 1%;
  }

  @media (max-width: 900px) {
    .lists {
      grid-template-columns: 1fr;
    }
  }
</style>
