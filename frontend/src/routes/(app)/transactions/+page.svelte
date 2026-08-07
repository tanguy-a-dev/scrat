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
    type OperationKind,
    type TransactionDto,
    type TransactionFilters,
    type TransactionSortField,
    type RangeMode,
  } from "$lib/api";
  import ImportCsvDialog from "$lib/ImportCsvDialog.svelte";
  import Checkbox from "$lib/Checkbox.svelte";
  import DeleteButton from "$lib/DeleteButton.svelte";
  import SearchSelect from "$lib/SearchSelect.svelte";
  import FilterPopover from "$lib/FilterPopover.svelte";
  import DateRangePicker from "$lib/DateRangePicker.svelte";
  import DatePicker from "$lib/DatePicker.svelte";
  import { pageViewState } from "$lib/pageCache";
  import { toast } from "$lib/toasts.svelte";
  import {
    ArrowUp,
    ChevronDown,
    FileUp,
    Pencil,
    Plus,
    RotateCcw,
    Search,
  } from "@lucide/svelte";

  /** The width at or below which the two lists stack instead of sitting side
   * by side, and so the width at or below which collapsing one of them means
   * anything. Side by side both are on screen at once and there is nothing to
   * collapse *for*.
   *
   * Duplicated in the `@media (max-width: 1600px)` rule at the bottom of this
   * file — CSS can't read a JS constant and JS can't read a media query, so
   * the two have to be moved together. */
  const STACKED_MAX_WIDTH_PX = 1600;

  function autofocus(node: HTMLElement) {
    node.focus();
  }

  /** Every operation kind the domain knows about, in the order shown in the
   * Type filter dropdown — kept here rather than derived from data, since a
   * kind with no transactions yet must still be choosable as a filter. */
  const OPERATION_KINDS: OperationKind[] = [
    "card",
    "bank_transfer",
    "direct_debit",
    "check",
    "cash",
    "fees",
    "other",
  ];

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
    // Expenses and Income keep independent sort state — sorting one list by
    // a column must never reorder the other.
    expenseSortField: "date" as SortField,
    expenseSortDir: "desc" as "asc" | "desc",
    incomeSortField: "date" as SortField,
    incomeSortDir: "desc" as "asc" | "desc",
    // Expenses and Income keep independent filters — a search or filter set
    // on one list must never narrow the other.
    expenseCategoryFilter: "",
    expenseDescriptionFilter: "",
    expenseAccountFilter: "",
    expenseTypeFilter: "",
    expenseMinAmount: "",
    expenseMaxAmount: "",
    incomeCategoryFilter: "",
    incomeDescriptionFilter: "",
    incomeAccountFilter: "",
    incomeTypeFilter: "",
    incomeMinAmount: "",
    incomeMaxAmount: "",
    // Which lists the user has folded away. Remembered like sort and filters
    // — it's a statement about how they want this page set up, not something
    // that should quietly undo itself on the next visit.
    expenseCollapsed: false,
    incomeCollapsed: false,
    loadedExpenseRows: 0,
    loadedIncomeRows: 0,
    scrollY: 0,
  }));

  /** A "go to transactions" jump from the Details page, as URL params.
   *
   * Params rather than reaching across to write this page's view cache from
   * over there: it keeps the origin a real `<a href>` (middle-click,
   * keyboard, copyable) and keeps the hand-off inspectable — same choice as
   * the `?action=` hand-off from the command palette below.
   *
   * Everything is validated. These are URL params, so nothing here may
   * assume the app is what wrote them: an unrecognised range or a
   * malformed id is dropped rather than passed down to a query. */
  function incomingFilter() {
    const params = page.url.searchParams;
    const kind = params.get("kind");
    if (kind !== "expense" && kind !== "income") return null;
    // A malformed id would reach `CategoryId::parse` on the Rust side and
    // fail the whole load with a parse error; a well-formed one naming
    // nothing just matches no rows, which is a survivable way to be wrong.
    const category = params.get("category") ?? "";
    if (!/^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(category)) return null;
    const range = params.get("range");
    const isoDate = (raw: string | null) =>
      raw && /^\d{4}-\d{2}-\d{2}$/.test(raw) ? raw : null;
    return {
      kind: kind as SelectionKind,
      category,
      mode:
        range === "month" || range === "year" || range === "all" || range === "custom"
          ? (range as RangeMode)
          : null,
      start: isoDate(params.get("start")),
      end: isoDate(params.get("end")),
    };
  }

  /** Applied to the cache rather than to the `$state` below, and read
   * synchronously at init: the declarations that follow already pick it up,
   * so `onMount(load)` issues the filtered fetch directly instead of an
   * unfiltered one it immediately has to redo. Writing the cache also means
   * navigating away and back returns to the filtered view, which is what
   * arriving here deliberately should leave behind.
   *
   * The target list's *other* filters are cleared. A jump is a statement
   * about what the user wants to look at now, and a description filter left
   * over from a visit ten minutes ago would silently intersect with it —
   * landing on an empty list with no visible reason why. The other list is
   * left completely alone; the two never share filters. */
  const incoming = incomingFilter();
  if (incoming) {
    if (incoming.mode) view.rangeMode = incoming.mode;
    if (incoming.start) view.customStart = incoming.start;
    if (incoming.end) view.customEnd = incoming.end;
    if (incoming.kind === "expense") {
      view.expenseCategoryFilter = incoming.category;
      view.expenseDescriptionFilter = "";
      view.expenseAccountFilter = "";
      view.expenseTypeFilter = "";
      view.expenseMinAmount = "";
      view.expenseMaxAmount = "";
    } else {
      view.incomeCategoryFilter = incoming.category;
      view.incomeDescriptionFilter = "";
      view.incomeAccountFilter = "";
      view.incomeTypeFilter = "";
      view.incomeMinAmount = "";
      view.incomeMaxAmount = "";
    }
    // The remembered scroll position was measured against a different set of
    // rows — restoring it would drop the user somewhere arbitrary in a list
    // they have never seen.
    view.scrollY = 0;
    view.loadedExpenseRows = 0;
    view.loadedIncomeRows = 0;
  }

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
  // "All transactions loaded" is a claim about what's on screen, so a folded
  // list counts as settled rather than as still-loading — otherwise
  // collapsing one list would leave the footer permanently blank, since that
  // list deliberately stops paging while it's away.
  let allTimeExhausted = $derived(
    (expenseExhausted || isCollapsed("expense")) &&
      (incomeExhausted || isCollapsed("income")),
  );
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

  type SortField = "date" | "amount" | "description" | "type" | "category" | "account";
  let expenseSortField = $state<SortField>(view.expenseSortField);
  let expenseSortDir = $state<"asc" | "desc">(view.expenseSortDir);
  let incomeSortField = $state<SortField>(view.incomeSortField);
  let incomeSortDir = $state<"asc" | "desc">(view.incomeSortDir);
  let expenseCategoryFilter = $state(view.expenseCategoryFilter);
  let expenseDescriptionFilter = $state(view.expenseDescriptionFilter);
  let expenseAccountFilter = $state(view.expenseAccountFilter);
  let expenseTypeFilter = $state<OperationKind | "">(
    view.expenseTypeFilter as OperationKind | "",
  );
  let expenseMinAmount = $state(view.expenseMinAmount);
  let expenseMaxAmount = $state(view.expenseMaxAmount);
  let incomeCategoryFilter = $state(view.incomeCategoryFilter);
  let incomeDescriptionFilter = $state(view.incomeDescriptionFilter);
  let incomeAccountFilter = $state(view.incomeAccountFilter);
  let incomeTypeFilter = $state<OperationKind | "">(
    view.incomeTypeFilter as OperationKind | "",
  );
  let incomeMinAmount = $state(view.incomeMinAmount);
  let incomeMaxAmount = $state(view.incomeMaxAmount);
  let expenseCollapsed = $state(view.expenseCollapsed);
  let incomeCollapsed = $state(view.incomeCollapsed);

  /** Whether the page is in its stacked, one-list-above-the-other layout —
   * compared against the same breakpoint the stylesheet uses, so the collapse
   * control appears exactly when the stacking it exists to relieve does.
   *
   * Measured from `innerWidth` (bound on `<svelte:window>` below), which is
   * the same width a `max-width` media query resolves against — both include
   * the scrollbar gutter, so the two can't disagree about which side of the
   * breakpoint the window is on. `matchMedia` with a `change` listener would
   * work too; this needs no listener to add and tear down, and `$derived`
   * only propagates when the boolean actually flips, so binding a value that
   * changes on every resize tick costs nothing downstream.
   *
   * The `> 0` guard is for the first frame, before the binding is
   * initialised: an unguarded 0 satisfies "≤ 1600" and would flash the
   * toggles onto a wide window. */
  let viewportWidth = $state(0);
  let stackedLayout = $derived(
    viewportWidth > 0 && viewportWidth <= STACKED_MAX_WIDTH_PX,
  );

  /** Collapsed *and* actually folded away. Widening the window back to the
   * side-by-side layout shows both lists again without discarding the
   * user's choice, so narrowing it once more folds the same list back up
   * rather than making them re-collapse it every time they resize. */
  function isCollapsed(kind: SelectionKind): boolean {
    if (!stackedLayout) return false;
    return kind === "expense" ? expenseCollapsed : incomeCollapsed;
  }

  function toggleCollapsed(kind: SelectionKind) {
    if (kind === "expense") expenseCollapsed = !expenseCollapsed;
    else incomeCollapsed = !incomeCollapsed;
    // A list that was folded away while "All Time" was paging stopped
    // fetching at whatever it had (see `loadMoreKind`), and unfolding it may
    // put the scroll sentinel back on screen with nothing under it. Top it
    // up rather than leave it stuck short until the user scrolls.
    if (!isCollapsed(kind)) fillViewportKind(kind);
  }

  // Mirrors the user's choices back into the cache. `loadedExpenseRows` /
  // `loadedIncomeRows` only mean anything in "All Time" — every other range
  // fetches its rows in one shot, so there's no page count to restore.
  $effect(() => {
    view.rangeMode = rangeMode;
    view.customStart = customStart;
    view.customEnd = customEnd;
    view.expenseSortField = expenseSortField;
    view.expenseSortDir = expenseSortDir;
    view.incomeSortField = incomeSortField;
    view.incomeSortDir = incomeSortDir;
    view.expenseCategoryFilter = expenseCategoryFilter;
    view.expenseDescriptionFilter = expenseDescriptionFilter;
    view.expenseAccountFilter = expenseAccountFilter;
    view.expenseTypeFilter = expenseTypeFilter;
    view.expenseMinAmount = expenseMinAmount;
    view.expenseMaxAmount = expenseMaxAmount;
    view.incomeCategoryFilter = incomeCategoryFilter;
    view.incomeDescriptionFilter = incomeDescriptionFilter;
    view.incomeAccountFilter = incomeAccountFilter;
    view.incomeTypeFilter = incomeTypeFilter;
    view.incomeMinAmount = incomeMinAmount;
    view.incomeMaxAmount = incomeMaxAmount;
    view.expenseCollapsed = expenseCollapsed;
    view.incomeCollapsed = incomeCollapsed;
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

  // The params have done their job at init. Dropping them keeps a reload
  // from re-clearing filters the user has since set by hand, and matches how
  // the `?action=` hand-off tidies up after itself.
  onMount(() => {
    if (incoming) replaceState(page.url.pathname, {});
  });

  /** Parses a user-typed amount filter bound into minor units, or `null`
   * when blank or unparseable — a filter box left empty or mid-edit means
   * "no bound", not zero. Always non-negative: bounds compare against the
   * transaction's magnitude, and a negative bound would silently exclude
   * everything. */
  function amountBoundMinorUnits(raw: string): number | null {
    const trimmed = raw.trim();
    if (!trimmed) return null;
    const parsed = parseToMinorUnits(trimmed);
    if (parsed === null) return null;
    return Math.abs(parsed);
  }

  /** A list's filters in the shape the backend takes them — every field
   * `null` means "no filter", not "match nothing". `isIncome` is baked in
   * from `kind` itself, since the Expenses and Income lists never mix
   * signs. */
  function activeFilters(kind: SelectionKind): TransactionFilters {
    const category = kind === "expense" ? expenseCategoryFilter : incomeCategoryFilter;
    const description =
      kind === "expense" ? expenseDescriptionFilter : incomeDescriptionFilter;
    const account = kind === "expense" ? expenseAccountFilter : incomeAccountFilter;
    const type = kind === "expense" ? expenseTypeFilter : incomeTypeFilter;
    const minAmount = kind === "expense" ? expenseMinAmount : incomeMinAmount;
    const maxAmount = kind === "expense" ? expenseMaxAmount : incomeMaxAmount;
    return {
      categoryId: category || null,
      descriptionContains: description.trim() || null,
      isIncome: kind === "income",
      accountId: account || null,
      operationKind: type || null,
      minAmountMinorUnits: amountBoundMinorUnits(minAmount),
      maxAmountMinorUnits: amountBoundMinorUnits(maxAmount),
    };
  }

  /** A list's sort in the shape `listTransactionsPage` takes — the one
   * `SortField` value that isn't already the backend's own word for it is
   * "type", which the backend (and its `TransactionSortField::parse`) calls
   * "operation_kind". */
  function activeSort(kind: SelectionKind): { field: TransactionSortField; dir: "asc" | "desc" } {
    const field = kind === "expense" ? expenseSortField : incomeSortField;
    const dir = kind === "expense" ? expenseSortDir : incomeSortDir;
    return { field: field === "type" ? "operation_kind" : field, dir };
  }

  /** Identity of the filter set a list's rows on screen were fetched with,
   * so the debounced effect below can tell a real filter change from its
   * own first run after `load()` already fetched with these same values. */
  function filterKey(kind: SelectionKind): string {
    return JSON.stringify(activeFilters(kind));
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
        const expenseSort = activeSort("expense");
        const incomeSort = activeSort("income");
        const [expenseBatch, incomeBatch] = await Promise.all([
          api.listTransactionsPage(
            0,
            expenseLimit,
            expenseFilters,
            expenseSort.field,
            expenseSort.dir,
          ),
          api.listTransactionsPage(0, incomeLimit, incomeFilters, incomeSort.field, incomeSort.dir),
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
   * filters and sort. A filter or sort change there is a different query,
   * not a narrowing or reordering of what's already on screen: the matching
   * rows (in the new order) can live anywhere in the ledger, including pages
   * that were never fetched — which is why filtering only the batches
   * loaded so far made a filter look like it had found almost nothing until
   * the whole ledger had been scrolled in, and the same reasoning is why
   * sorting only those batches found the wrong "highest"/"lowest" row.
   * Deliberately doesn't touch `loading`: that would unmount the table, and
   * with it the filter/sort control the user is still interacting with. */
  async function reloadFilteredAllTimeKind(kind: SelectionKind) {
    const token = bumpToken(kind);
    const filters = activeFilters(kind);
    const sort = activeSort(kind);
    if (kind === "expense") loadingMoreExpense = true;
    else loadingMoreIncome = true;
    try {
      const batch = await api.listTransactionsPage(
        0,
        PAGE_SIZE,
        filters,
        sort.field,
        sort.dir,
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

  /** Paging a folded-away list isn't just wasted work, it's unbounded: the
   * two lists share one scroll sentinel, a collapsed list adds no height to
   * the page, so every batch it pulls leaves the sentinel exactly where it
   * was and `fillViewportKind`'s loop never reaches its stopping condition.
   * Collapsing Expenses to read Income would quietly fetch the entire
   * expense ledger — the single decades-long query this pagination exists to
   * avoid. Both the loop and the single-batch fetch check, because the
   * IntersectionObserver can call the latter on its own. */
  async function loadMoreKind(kind: SelectionKind) {
    const exhausted = kind === "expense" ? expenseExhausted : incomeExhausted;
    const alreadyLoading =
      kind === "expense" ? loadingMoreExpense : loadingMoreIncome;
    if (rangeMode !== "all" || exhausted || alreadyLoading || isCollapsed(kind)) {
      return;
    }
    const token = currentToken(kind);
    if (kind === "expense") loadingMoreExpense = true;
    else loadingMoreIncome = true;
    try {
      const filters = activeFilters(kind);
      const sort = activeSort(kind);
      const offset = kind === "expense" ? expenseOffset : incomeOffset;
      const batch = await api.listTransactionsPage(
        offset,
        PAGE_SIZE,
        filters,
        sort.field,
        sort.dir,
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
      !isCollapsed(kind) &&
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
      const count = await api.countTransactions(
        currentRange.start,
        currentRange.end,
        activeFilters(kind),
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

  let accountOptions = $derived(accounts.map((a) => ({ id: a.id, label: a.name })));
  let accountFilterOptions = $derived([
    { id: "", label: "All accounts" },
    ...accountOptions,
  ]);

  function accountFilterFor(kind: SelectionKind): string {
    return kind === "expense" ? expenseAccountFilter : incomeAccountFilter;
  }

  function setAccountFilter(kind: SelectionKind, id: string) {
    if (kind === "expense") expenseAccountFilter = id;
    else incomeAccountFilter = id;
  }

  let typeFilterOptions = $derived([
    { id: "", label: "All types" },
    ...OPERATION_KINDS.map((kind) => ({ id: kind, label: operationKindLabel(kind) })),
  ]);

  function typeFilterFor(kind: SelectionKind): string {
    return kind === "expense" ? expenseTypeFilter : incomeTypeFilter;
  }

  function setTypeFilter(kind: SelectionKind, id: string) {
    const value = id as OperationKind | "";
    if (kind === "expense") expenseTypeFilter = value;
    else incomeTypeFilter = value;
  }

  function minAmountFilterFor(kind: SelectionKind): string {
    return kind === "expense" ? expenseMinAmount : incomeMinAmount;
  }

  function maxAmountFilterFor(kind: SelectionKind): string {
    return kind === "expense" ? expenseMaxAmount : incomeMaxAmount;
  }

  function setMinAmountFilter(kind: SelectionKind, value: string) {
    if (kind === "expense") expenseMinAmount = value;
    else incomeMinAmount = value;
  }

  function setMaxAmountFilter(kind: SelectionKind, value: string) {
    if (kind === "expense") expenseMaxAmount = value;
    else incomeMaxAmount = value;
  }

  function amountFilterActive(kind: SelectionKind): boolean {
    return minAmountFilterFor(kind).trim() !== "" || maxAmountFilterFor(kind).trim() !== "";
  }

  /** Whether this list is still showing the view it opens with: newest
   * first, nothing narrowed. Drives whether the reset control is offered at
   * all — an always-visible button next to a pristine list is a control that
   * does nothing. The text boxes are compared raw rather than trimmed:
   * a lone space filters nothing but is still text sitting in the box, and
   * the user needs a way to clear it. */
  function viewIsDefault(kind: SelectionKind): boolean {
    const sortField = kind === "expense" ? expenseSortField : incomeSortField;
    const sortDir = kind === "expense" ? expenseSortDir : incomeSortDir;
    return (
      sortField === "date" &&
      sortDir === "desc" &&
      categoryFilterFor(kind) === "" &&
      descriptionFilterFor(kind) === "" &&
      accountFilterFor(kind) === "" &&
      typeFilterFor(kind) === "" &&
      minAmountFilterFor(kind) === "" &&
      maxAmountFilterFor(kind) === ""
    );
  }

  /** Puts one list back to its default view. Only touches that list's state —
   * Expenses and Income keep independent sort and filters, so resetting one
   * must leave the other exactly as the user left it. Clearing the filters
   * changes `filterKey`, which the debounced effect above already treats as
   * a filter change (refreshing the count and, in "All Time", re-querying
   * the rows) — but that effect only fires on a `filterKey` change, so a
   * reset that touches only the sort (filters already default) would
   * otherwise snap the sort controls back to "Date" on screen while leaving
   * the stale, differently-ordered pages already fetched in place. Fetching
   * here directly, and marking the key as already applied, covers that case
   * without asking the debounced effect to fire a redundant second fetch
   * when both filters and sort actually changed. */
  function resetView(kind: SelectionKind) {
    if (kind === "expense") {
      expenseSortField = "date";
      expenseSortDir = "desc";
      expenseCategoryFilter = "";
      expenseDescriptionFilter = "";
      expenseAccountFilter = "";
      expenseTypeFilter = "";
      expenseMinAmount = "";
      expenseMaxAmount = "";
    } else {
      incomeSortField = "date";
      incomeSortDir = "desc";
      incomeCategoryFilter = "";
      incomeDescriptionFilter = "";
      incomeAccountFilter = "";
      incomeTypeFilter = "";
      incomeMinAmount = "";
      incomeMaxAmount = "";
    }
    if (kind === "expense") appliedExpenseFilterKey = filterKey("expense");
    else appliedIncomeFilterKey = filterKey("income");
    refreshCount(kind);
    if (rangeMode === "all") reloadFilteredAllTimeKind(kind);
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

  /** "All Time" fetches `expenseRows`/`incomeRows` already filtered by the
   * backend — unlike every other range, which re-derives its filtered list
   * from `transactions` on every change. Recategorizing there mutates
   * `category_id` in place but never removes the row, so a transaction that
   * no longer matches an active category filter stays visible until the
   * list reloads. Re-applying `matchesFilters` here is what the other
   * ranges get for free from their `$derived.by`. The offset is a backend
   * fetch cursor, not derived from array length, so it has to shrink by
   * however many rows were just dropped — same reasoning as the bulk-delete
   * offset adjustment above. */
  function pruneAllTimeRows(kind: SelectionKind) {
    const filters = activeFilters(kind);
    if (kind === "expense") {
      const before = expenseRows.length;
      expenseRows = expenseRows.filter((tx) => matchesFilters(tx, filters));
      expenseOffset -= before - expenseRows.length;
    } else {
      const before = incomeRows.length;
      incomeRows = incomeRows.filter((tx) => matchesFilters(tx, filters));
      incomeOffset -= before - incomeRows.length;
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
      const kind = t.amount_minor_units < 0 ? "expense" : "income";
      if (rangeMode === "all") pruneAllTimeRows(kind);
      // Recategorizing can move this transaction in or out of an active
      // category filter's count — only ever the count for its own sign.
      refreshCount(kind);
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

  /** In "All Time", the rows on screen are only a prefix of the ledger
   * fetched in whatever order was active when they were paged in — changing
   * that order makes them the wrong prefix, since the true top-N under the
   * new sort can include rows that were never fetched at all. So a sort
   * change there restarts that list's pagination from offset 0 under the
   * new sort, the same way a filter change does; every other range already
   * has the whole (unfiltered) range loaded, so re-sorting the rows already
   * on screen is enough. */
  function toggleSort(kind: SelectionKind, field: SortField) {
    if (kind === "expense") {
      if (expenseSortField === field) {
        expenseSortDir = expenseSortDir === "asc" ? "desc" : "asc";
      } else {
        expenseSortField = field;
        expenseSortDir = "desc";
      }
    } else {
      if (incomeSortField === field) {
        incomeSortDir = incomeSortDir === "asc" ? "desc" : "asc";
      } else {
        incomeSortField = field;
        incomeSortDir = "desc";
      }
    }
    if (rangeMode === "all") reloadFilteredAllTimeKind(kind);
  }

  function sortTransactions(list: TransactionDto[], kind: SelectionKind): TransactionDto[] {
    const sortField = kind === "expense" ? expenseSortField : incomeSortField;
    const sortDir = kind === "expense" ? expenseSortDir : incomeSortDir;
    return [...list].sort((a, b) => {
      let cmp = 0;
      if (sortField === "date") cmp = a.date.localeCompare(b.date);
      else if (sortField === "amount")
        cmp = a.amount_minor_units - b.amount_minor_units;
      else if (sortField === "description") cmp = a.description.localeCompare(b.description);
      else if (sortField === "type")
        cmp = operationKindLabel(a.operation_kind).localeCompare(
          operationKindLabel(b.operation_kind),
        );
      else if (sortField === "account")
        cmp = accountName(a.account_id).localeCompare(accountName(b.account_id));
      else
        cmp = categoryName(a.category_id).localeCompare(
          categoryName(b.category_id),
        );
      return sortDir === "asc" ? cmp : -cmp;
    });
  }

  /** The same predicate the backend applies for "All Time", re-derived here
   * for the other ranges — those fetch the whole (unfiltered, mixed-sign)
   * range in one shot and filter it client-side, so this has to agree with
   * `TransactionFilters` field for field rather than reimplementing it. */
  function matchesFilters(t: TransactionDto, filters: TransactionFilters): boolean {
    if (filters.categoryId && t.category_id !== filters.categoryId) return false;
    if (
      filters.descriptionContains &&
      !t.description.toLowerCase().includes(filters.descriptionContains.toLowerCase())
    ) {
      return false;
    }
    if (filters.accountId && t.account_id !== filters.accountId) return false;
    if (filters.operationKind && t.operation_kind !== filters.operationKind) return false;
    const magnitude = Math.abs(t.amount_minor_units);
    if (filters.minAmountMinorUnits !== null && magnitude < filters.minAmountMinorUnits) {
      return false;
    }
    if (filters.maxAmountMinorUnits !== null && magnitude > filters.maxAmountMinorUnits) {
      return false;
    }
    return true;
  }

  // "All Time" already fetches each list pre-filtered and sign-narrowed
  // from the backend — `expenseRows`/`incomeRows` need only sorting. Every
  // other range fetches the whole (unfiltered, mixed-sign) range in one
  // shot, so this list applies its own filter and sign split client-side.
  let expenses = $derived.by(() => {
    if (rangeMode === "all") return sortTransactions(expenseRows, "expense");
    const filters = activeFilters("expense");
    return sortTransactions(
      transactions.filter((t) => t.amount_minor_units < 0 && matchesFilters(t, filters)),
      "expense",
    );
  });
  let income = $derived.by(() => {
    if (rangeMode === "all") return sortTransactions(incomeRows, "income");
    const filters = activeFilters("income");
    return sortTransactions(
      transactions.filter((t) => t.amount_minor_units > 0 && matchesFilters(t, filters)),
      "income",
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
      const expenseCountBefore = expenseRows.length;
      const incomeCountBefore = incomeRows.length;
      transactions = transactions.filter(survives);
      expenseRows = expenseRows.filter(survives);
      incomeRows = incomeRows.filter(survives);
      // expenseOffset/incomeOffset are backend fetch cursors, not derived from
      // array length — removing already-loaded rows shifts every later
      // backend row up by the number removed, so the cursor must shrink by
      // that same amount or the next page fetch skips the rows that just
      // moved into the gap (and can wrongly mark the list "exhausted" if the
      // skipped-past fetch comes back short).
      expenseOffset -= expenseCountBefore - expenseRows.length;
      incomeOffset -= incomeCountBefore - incomeRows.length;
      selectionSet(kind).clear();
      setLastClickedId(kind, null);
      // A transfer's counterpart can land in either list's count, so both
      // are refreshed regardless of which list the selection was made in.
      await Promise.all([refreshCount("expense"), refreshCount("income")]);
      // The IntersectionObserver only fires on an intersection *change*; a
      // sentinel that stays in view after rows vanish from under it would
      // otherwise never re-trigger, leaving the list stuck empty.
      await fillViewport();
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
      if (rangeMode === "all") pruneAllTimeRows(kind);
      // Only matters when this list's category filter is active — for every
      // range but "All Time" the `$derived.by` above drops rows that no
      // longer match it on its own; `pruneAllTimeRows` just did the same
      // job for "All Time".
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
<svelte:window onmouseup={endRowDrag} bind:innerWidth={viewportWidth} />

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
            ><button type="button" onclick={() => toggleSort(kind, "date")}
              >Date</button
            ></th
          >
          <th class="amount-cell">
            <div class="column-header" class:filtered={amountFilterActive(kind)}>
              <button type="button" onclick={() => toggleSort(kind, "amount")}
                >Amount</button
              >
              <FilterPopover
                active={amountFilterActive(kind)}
                ariaLabel="Filter by amount"
              >
                <div class="amount-filter">
                  <label>
                    Min
                    <input
                      type="number"
                      step="0.01"
                      min="0"
                      value={minAmountFilterFor(kind)}
                      oninput={(e) =>
                        setMinAmountFilter(kind, (e.currentTarget as HTMLInputElement).value)}
                      use:autofocus
                      placeholder="0.00"
                    />
                  </label>
                  <label>
                    Max
                    <input
                      type="number"
                      step="0.01"
                      min="0"
                      value={maxAmountFilterFor(kind)}
                      oninput={(e) =>
                        setMaxAmountFilter(kind, (e.currentTarget as HTMLInputElement).value)}
                      placeholder="0.00"
                    />
                  </label>
                </div>
              </FilterPopover>
            </div>
          </th>
          <th class="description-cell">
            <div
              class="column-header"
              class:filtered={descriptionFilterFor(kind).trim() !== ""}
            >
              <button type="button" onclick={() => toggleSort(kind, "description")}
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
          <th class="kind-cell">
            <div class="column-header" class:filtered={typeFilterFor(kind) !== ""}>
              <button type="button" onclick={() => toggleSort(kind, "type")}>Type</button>
              <SearchSelect
                options={typeFilterOptions}
                value={typeFilterFor(kind)}
                onChange={(id) => setTypeFilter(kind, id)}
                searchPlaceholder="Search type…"
              >
                {#snippet trigger()}
                  <Search size={14} aria-label="Filter by type" />
                {/snippet}
              </SearchSelect>
            </div>
          </th>
          <th class="category-cell">
            <div
              class="column-header align-right"
              class:filtered={categoryFilterFor(kind) !== ""}
            >
              <button type="button" onclick={() => toggleSort(kind, "category")}
                >Category</button
              >
              <SearchSelect
                options={categoryFilterOptions}
                value={categoryFilterFor(kind)}
                onChange={(id) => setCategoryFilter(kind, id)}
                searchPlaceholder="Search category…"
              >
                {#snippet trigger()}
                  <Search size={14} aria-label="Filter by category" />
                {/snippet}
              </SearchSelect>
            </div>
          </th>
          <th class="account-cell">
            <div
              class="column-header align-right"
              class:filtered={accountFilterFor(kind) !== ""}
            >
              <button type="button" onclick={() => toggleSort(kind, "account")}>Account</button>
              <SearchSelect
                options={accountFilterOptions}
                value={accountFilterFor(kind)}
                onChange={(id) => setAccountFilter(kind, id)}
                searchPlaceholder="Search account…"
              >
                {#snippet trigger()}
                  <Search size={14} aria-label="Filter by account" />
                {/snippet}
              </SearchSelect>
            </div>
          </th>
          <th class="actions-cell"></th>
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
              <td class="amount-cell">{formatCurrency(t.amount_minor_units, t.currency)}</td>
              <td class="description-cell">
                {t.description}
                {#if t.role === "transfer"}
                  <span class="role-badge" title="Between your own accounts — not counted as spending"
                    >transfer</span
                  >
                {:else if t.role === "adjustment"}
                  <span
                    class="role-badge"
                    title="Balance adjustment, posted from Accounts — not counted as spending"
                    >adjustment</span
                  >
                {/if}
              </td>
              <td class="kind-cell">{operationKindLabel(t.operation_kind)}</td>
              <td class="category-cell">
                <SearchSelect
                  options={categoryOptions}
                  value={t.category_id}
                  onChange={(categoryId) => handleCategoryChange(t, categoryId)}
                  searchPlaceholder="Search category…"
                  stacked
                />
              </td>
              <td class="account-cell">{accountName(t.account_id)}</td>
              <td class="actions-cell">
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
    <DatePicker value={formDate} onChange={(d) => (formDate = d)} />
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
    <SearchSelect
      options={categoryOptions}
      value={formCategoryId}
      onChange={(id) => (formCategoryId = id)}
      placeholder="Category…"
      searchPlaceholder="Search category…"
    />
    <SearchSelect
      options={accountOptions}
      value={formAccountId}
      onChange={(id) => (formAccountId = id)}
      placeholder="Account…"
      searchPlaceholder="Search account…"
    />
    <button type="submit">Save transaction</button>
  </form>
{/if}

{#if loading}
  <p>Loading…</p>
{:else}
  <div class="lists">
    <section>
      <div class="section-header">
        <h2>
          {#if stackedLayout}
            <button
              type="button"
              class="collapse-toggle"
              aria-expanded={!expenseCollapsed}
              aria-controls="expenses-list"
              title={expenseCollapsed ? "Show expenses" : "Hide expenses"}
              onclick={() => toggleCollapsed("expense")}
            >
              <span class="chevron" class:folded={expenseCollapsed} aria-hidden="true">
                <ChevronDown size={16} />
              </span>
              Expenses
            </button>
          {:else}
            Expenses
          {/if}
        </h2>
        {#if isCollapsed("expense")}
          <span class="collapsed-count">{expenseCount} transactions</span>
        {/if}
        {#if !viewIsDefault("expense")}
          <button
            type="button"
            class="reset-view"
            aria-label="Reset expenses sort and filters"
            title="Reset sort and filters"
            onclick={() => resetView("expense")}
          >
            <RotateCcw size={13} aria-hidden="true" />
            Reset
          </button>
        {/if}
        {#if visibleSelectedExpenseIds.length > 0}
          <div
            class="bulk-actions"
            role="toolbar"
            aria-label="Bulk actions for expenses"
            aria-live="polite"
          >
            <span class="bulk-count">{visibleSelectedExpenseIds.length} selected</span>
            <SearchSelect
              options={categoryOptions}
              value=""
              onChange={(id) => handleBulkRecategorize("expense", id)}
              searchPlaceholder="Search category…"
            >
              {#snippet trigger()}
                <Pencil size={14} aria-label="Recategorize selected expenses" />
              {/snippet}
            </SearchSelect>
            <DeleteButton
              compact
              label={`Delete ${visibleSelectedExpenseIds.length} transactions`}
              onConfirm={() => handleBulkDelete("expense")}
            />
          </div>
        {/if}
      </div>
      <div id="expenses-list">
        {#if !isCollapsed("expense")}
          {@render list(expenses, "expense")}
        {/if}
      </div>
    </section>
    <section>
      <div class="section-header">
        <h2>
          {#if stackedLayout}
            <button
              type="button"
              class="collapse-toggle"
              aria-expanded={!incomeCollapsed}
              aria-controls="income-list"
              title={incomeCollapsed ? "Show income" : "Hide income"}
              onclick={() => toggleCollapsed("income")}
            >
              <span class="chevron" class:folded={incomeCollapsed} aria-hidden="true">
                <ChevronDown size={16} />
              </span>
              Income
            </button>
          {:else}
            Income
          {/if}
        </h2>
        {#if isCollapsed("income")}
          <span class="collapsed-count">{incomeCount} transactions</span>
        {/if}
        {#if !viewIsDefault("income")}
          <button
            type="button"
            class="reset-view"
            aria-label="Reset income sort and filters"
            title="Reset sort and filters"
            onclick={() => resetView("income")}
          >
            <RotateCcw size={13} aria-hidden="true" />
            Reset
          </button>
        {/if}
        {#if visibleSelectedIncomeIds.length > 0}
          <div
            class="bulk-actions"
            role="toolbar"
            aria-label="Bulk actions for income"
            aria-live="polite"
          >
            <span class="bulk-count">{visibleSelectedIncomeIds.length} selected</span>
            <SearchSelect
              options={categoryOptions}
              value=""
              onChange={(id) => handleBulkRecategorize("income", id)}
              searchPlaceholder="Search category…"
            >
              {#snippet trigger()}
                <Pencil size={14} aria-label="Recategorize selected income" />
              {/snippet}
            </SearchSelect>
            <DeleteButton
              compact
              label={`Delete ${visibleSelectedIncomeIds.length} transactions`}
              onConfirm={() => handleBulkDelete("income")}
            />
          </div>
        {/if}
      </div>
      <div id="income-list">
        {#if !isCollapsed("income")}
          {@render list(income, "income")}
        {/if}
      </div>
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

  /* SearchSelect renders its own scoped trigger button (narrower padding,
     smaller font, shrink-to-fit width) — sized fine for a table-header
     filter icon, but next to the plain Amount/Description inputs here it
     reads as a visibly smaller field. Matched to `input`'s own box below
     rather than left to inherit it, since Svelte's scoped styles don't
     cross into a child component's markup. */
  .create-form :global(.search-select) {
    width: 10.5rem;
    max-width: none;
  }

  .create-form :global(.search-select .trigger) {
    border-radius: 6px;
    padding: 0.45rem 0.5rem;
    font-size: 0.9rem;
  }

  input,
  button:not(.icon-button) {
    border-radius: 6px;
    border: 1px solid var(--color-shade-3);
    padding: 0.45rem 0.5rem;
    font-size: 0.9rem;
    font-family: inherit;
  }

  input {
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

  /* `minmax(0, 1fr)`, not `1fr`. A bare `1fr` is `minmax(auto, 1fr)`, and
     `auto` as a *minimum* means min-content — so a table too wide to fit
     doesn't get compressed, it pushes its grid column (and the page) wider
     and hands the user a horizontal scrollbar. The `0` minimum lets each
     column shrink to the space actually available and forces the table to
     solve the fit instead, which the column rules below let it do. */
  .lists {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 1rem;
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

  /* Selector is `button.collapse-toggle`, not `.collapse-toggle`, for the
     same reason `.reset-view` needs it: the generic `button:not(.icon-button)`
     rule scores (0,1,1) and would otherwise win, handing this a border and a
     0.9rem font in place of the heading's own.

     The negative margin pulls the chevron left into the 2.5rem gutter that
     `.section-header` reserves for the checkbox column, so "Expenses" and
     "Income" keep the left edge they share with the "Date" heading below —
     adding the chevron in flow would have shifted both headings right and
     broken that alignment for the sake of an icon. */
  button.collapse-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    margin-left: -1.4rem;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .chevron {
    display: inline-flex;
    transition: transform 0.15s ease;
  }

  /* Points down at an open list and right at a folded one — the direction
     the content went, which is the convention every disclosure widget uses. */
  .chevron.folded {
    transform: rotate(-90deg);
  }

  /* A folded list says nothing about what's inside it; the count is the one
     thing worth keeping visible, so the fold is a considered choice rather
     than a guess about where the rows went. */
  .collapsed-count {
    font-size: 0.85rem;
    opacity: 0.6;
    white-space: nowrap;
  }

  /* Sits in the section header rather than in a `<th>`: that row already
     exists and already reserves 2rem of height for the bulk-actions pill, so
     this costs the page no extra height — and, unlike a control inside a
     header cell, nothing in the table's column widths either (see the
     `.column-header :global(.trigger)` note below for what an in-flow header
     control actually costs a column). It's per list because Expenses and
     Income keep independent sort and filters.

     Selector is `button.reset-view`, not `.reset-view`: the generic
     `button:not(.icon-button)` rule above scores (0,1,1) — `:not()` takes its
     argument's specificity — so a bare class would lose its padding and
     font-size to it. */
  button.reset-view {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.15rem 0.5rem;
    border: 1px solid var(--color-accent);
    border-radius: 999px;
    background: transparent;
    color: var(--color-accent);
    font-size: 0.75rem;
    cursor: pointer;
  }

  button.reset-view:hover {
    background-color: var(--color-shade-2);
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

  /* `fixed`, not the default `auto`. Auto layout sizes columns from their
     content, and a column's min-content width is a floor it will overflow
     the page rather than go under — which is how a half-width table with
     eight columns ended up wider than the window. Worse, that floor is set
     by whichever cell happens to hold the longest word, so the space went
     where the data fell rather than where it was useful: measured at a 760px
     window, Category (a dropdown showing "Courses alimentaires") held 151px
     while Description was squeezed to 64px.

     Fixed layout ignores content entirely and uses the widths declared
     below, so the table is always exactly as wide as the space it's given
     and the split between columns is a decision rather than an accident. */
  table {
    width: 100%;
    table-layout: fixed;
    border-collapse: collapse;
    font-size: 0.9rem;
  }

  th {
    text-align: left;
    padding: 0.3rem 0.4rem;
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

  /* `flex`, not `inline-flex`, so `max-width` has a definite box to bite on
     and the label below can be told to shrink. */
  .column-header {
    display: flex;
    align-items: center;
    gap: 0.15rem;
    max-width: 100%;
  }

  /* A column label is a single word, and a single word narrower than its
     column wraps one letter per line — at a 700px window "Account" rendered
     as a seven-line vertical stack that spilled over the Category header
     next to it. Clipping the label instead keeps the header one line tall at
     any width; the ellipsis says it's been shortened. `min-width: 0` is what
     actually lets it shrink: a flex item won't go below its own min-content
     (the whole word) without it. Not done by clipping the `th` — the filter
     dropdowns are positioned inside it and would be clipped too. */
  th .column-header > button,
  th > button {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Deliberately still in flow, not `position: absolute`. An earlier version
     took the trigger out of flow to make it free, but that only looked free
     — it actually bet on the column already having slack beyond the label's
     own width, and that bet doesn't hold. `table { width: 100% }` forces the
     browser to distribute extra width across columns to fill the row, and
     with real transaction rows on screen that extra goes disproportionately
     to Description and Category (Type already opts out via `.kind-cell`'s
     `width: 1%`), not evenly to every column. Amount, with only short
     numbers in it, can render at exactly its own label width with zero
     slack — at which point an out-of-flow icon has nowhere to sit but on
     top of Description. Confirmed live: with short amounts and real-length
     descriptions, the closed trigger measured 4–8px into the Description
     header. The empty-state screenshot never showed this because a single
     colspan row doesn't drive that uneven distribution, so Amount happened
     to get enough slack there by accident.

     Keeping the icon in flow means it counts toward the column's min-content
     width, which table auto-layout can add to but never shrink below — so
     the column is now guaranteed to always be at least "label + icon" wide,
     regardless of how the row data happens to distribute the rest. The cost
     is real (previously measured at ~76px per table over the fully free
     version) rather than the free lunch the absolute version promised, but
     it can't be spilled into a neighbour by a data shape this page didn't
     anticipate. */
  .column-header :global(.trigger) {
    padding: 3px;
    border: none;
    background: transparent;
    /* Faint, but not so faint the affordance is undiscoverable — this is
       now the only thing advertising that a column can be filtered at all,
       where before it was a bordered button. */
    opacity: 0.55;
  }

  .column-header :global(svg) {
    width: 10px;
    height: 10px;
  }

  th:hover .column-header :global(.trigger) {
    opacity: 1;
  }

  /* An active filter is now signalled by the column label, not by the icon.
     Losing the filled accent chip would otherwise leave a 10px tinted glyph
     as the only clue that a list is narrowed — and since the header count
     silently reports the filtered total, a user can read a filtered ledger
     without noticing. Tinting the label puts that signal on the widest,
     most-looked-at thing in the cell. */
  .column-header.filtered > button {
    color: var(--color-accent);
  }

  /* Both trigger components fill themselves with the accent colour when
     their filter is set. That chip read well while the icon was inline and
     bordered; at 10px it collapses into a solid block with no glyph left in
     it. The label carries the active signal now, so the icon only needs to
     take the accent colour.

     The selectors are deliberately this long: each component scopes its own
     `.trigger.icon-trigger.active` rule, which lands at the same
     specificity as a shorter version of this one, and a tie would be
     settled by whichever stylesheet the bundler happened to emit last. */
  .column-header :global(.trigger.active),
  .column-header :global(.trigger.icon-trigger.active) {
    background: transparent;
    border-color: transparent;
    color: var(--color-accent);
    opacity: 1;
  }

  /* Both dropdowns open with `left: 0`, which for the right-hand columns
     runs off the window — the Income table's Account panel measured 182px
     past the viewport at 1440px wide. Flipping the rightmost columns to
     open leftward keeps them on screen. */
  .column-header.align-right :global(.dropdown) {
    left: auto;
    right: 0;
  }

  .amount-filter {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.5rem 0.6rem;
  }

  .amount-filter label {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.75rem;
    opacity: 0.75;
  }

  .amount-filter input {
    width: 100%;
    box-sizing: border-box;
    border: 1px solid var(--color-shade-3);
    border-radius: 4px;
    background: var(--color-shade-2);
    color: inherit;
    padding: 0.35rem 0.5rem;
    font-size: 0.85rem;
    font-family: inherit;
  }

  td {
    padding: 0.2rem 0.35rem;
    border-bottom: 1px solid var(--color-shade-2);
  }

  /* Rigid columns, in absolute units: a date, an amount and an operation
     kind are short, fixed-shape strings whose width doesn't depend on how
     wide the window is, and each is sized to the widest value it can
     realistically hold. Giving them the window's surplus would only pad
     them out; it goes to the three text columns instead.

     Targeted by class, not :first-child — the checkbox column sits to the
     Date column's left now, and a position-based selector silently stops
     matching the moment the column order changes again (already bit us once
     when this column added the leading checkbox). */
  .date-cell {
    white-space: nowrap;
    width: 5.5rem;
  }

  /* Wide enough for a grouped amount plus a three-letter currency code, the
     fallback `formatCurrency` uses for anything without a symbol
     ("-CHF 25 000,00"). Deliberately not `nowrap`: fixed layout gives a
     column exactly its declared width and lets anything longer spill over
     the neighbouring cell, and an amount is the one thing on this page that
     must never be shown overlapping or half-read. Wrapping is the graceful
     failure — an implausibly large amount takes two lines and stays
     legible. */
  .amount-cell {
    width: 7rem;
    overflow-wrap: anywhere;
  }

  /* How the money moved. Reads exactly like Account — same colour, size and
     weight — since the two are the same kind of thing: flat context about
     the row, not something ranked above it. */
  .kind-cell {
    white-space: nowrap;
    width: 5.5rem;
  }

  /* Just wide enough for the round trash button. The confirm step that
     replaces it is drawn out of flow (see DeleteButton.svelte), so this
     width holds whether or not a row is mid-confirmation. */
  .actions-cell {
    width: 2.4rem;
    padding-left: 0.15rem;
    padding-right: 0.15rem;
  }

  /* The elastic columns: free text of unbounded length, so these are the
     ones that give way as the window narrows. The percentages are read as a
     ratio rather than as literal shares — they can't all be honoured once
     the rigid columns above have taken their absolute widths — so what they
     actually set is how the leftover space is split: Description gets the
     most because it's the only column that says what a transaction *was*. */
  .description-cell {
    width: 40%;
  }

  .category-cell {
    width: 32%;
  }

  .account-cell {
    width: 28%;
  }

  /* Bank descriptions are single unbroken tokens as often as not
     ("PRLV/SEPA/ABONNEMENT..."), and account names can be too. Without this
     they'd spill out of a column narrower than their longest word instead
     of wrapping inside it. */
  .description-cell,
  .account-cell {
    overflow-wrap: anywhere;
  }

  /* SearchSelect caps itself at 11rem, sized for the Add form where it's one
     of five controls on a full-width row. Here it should simply be as wide
     as the column it's in — wider when the window allows, narrower when it
     doesn't; the trigger already ellipsises its label either way.

     `td`, not the bare class: the Category *header* holds a SearchSelect
     too (the filter icon), and stretching that one to the full column width
     pushed the "Category" label down to "Ca…". */
  td.category-cell :global(.search-select) {
    display: block;
    width: 100%;
    max-width: 100%;
  }

  /* Side by side, each list gets a little under half the window; below this
     the eight columns still *fit* (the rules above guarantee that at any
     width) but stop being readable — every text column ellipsised, every
     description wrapping to three lines. So the two lists stack and each
     takes the full width instead.

     Measured, not guessed: Account is the narrowest elastic column and its
     own header needs ~90px (label, filter icon, cell padding), which it only
     gets once a table is ~700px wide — two of those plus the nav rail, the
     page padding, the grid gap and the divider is 1600px. Below that,
     stacking trades seeing both lists at once for seeing either one
     properly, which is the better trade for a ledger you read a row at a
     time. */
  @media (max-width: 1600px) {
    .lists {
      grid-template-columns: minmax(0, 1fr);
    }

    /* The divider is between two columns; stacked, it would be a stray line
       down the left of the second list. */
    .lists > section:last-child {
      border-left: none;
      padding-left: 0;
      /* Replaces the vertical rule with a horizontal one, so the two lists
         stay visibly separate now that they're above and below each other. */
      border-top: 0.5px solid var(--color-accent);
      padding-top: 1.5rem;
    }
  }

  /* Below this the elastic columns are narrower than their own one-word
     headers, so nothing is gained by keeping all eight — the table stops
     shrinking and starts becoming unreadable. Account goes first and Type
     next: both are flat context about a row, where Date, Amount,
     Description and Category are what identify and classify it.

     Hiding a column also hides its sort and filter control. That's a real
     loss, but it's bounded — the Reset button in the section header appears
     whenever any filter is set, including one left over from a wider window,
     so a filter can never become both invisible and unclearable. */
  @media (max-width: 820px) {
    .account-cell {
      display: none;
    }
  }

  @media (max-width: 680px) {
    .kind-cell {
      display: none;
    }
  }
</style>
