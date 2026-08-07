<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    countsTowardTotals,
    formatCurrency,
    computeRange,
    describeRange,
    formatDateSpan,
    precedingSpan,
    spanDays,
    todayIsoDate,
    oneMonthAgoIsoDate,
    type CategoryDto,
    type TransactionDto,
    type RangeMode,
  } from "$lib/api";
  import DateRangePicker from "$lib/DateRangePicker.svelte";
  import * as breakdown from "$lib/breakdown";
  import { pageViewState } from "$lib/pageCache";
  import {
    ArrowUpRight,
    ChevronLeft,
    ChevronRight,
    GitCompareArrows,
    TriangleAlert,
  } from "@lucide/svelte";

  // Validated categorical palette (dark-mode steps) — passes CVD/contrast
  // checks against this app's dark surface. See dataviz skill's palette.md.
  //
  // Slot 1 sits in the app's own hue family (OKLCH h≈182, the hue of
  // --color-accent and --color-box) and the rest walk outward from it,
  // alternating cool/warm — so a typical 3-5 category chart reads
  // teal/violet/coral rather than as generic default chart colors.
  // Lightness steps are tuned for the pairs that can actually touch here:
  // adjacent slices, plus the wrap pair where the last slice meets the first
  // at every category count. Worst adjacent CVD ΔE 14.0, normal-vision 25.7,
  // all slots inside the dark band and >= 3:1 on black.
  const PALETTE = [
    "#00ab99",
    "#a367df",
    "#ed613f",
    "#007fa9",
    "#b68d00",
    "#6a89ff",
    "#089428",
    "#bf489e",
  ];
  const RADIUS = 80;
  const CIRCUMFERENCE = 2 * Math.PI * RADIUS;
  const SLICE_WIDTH = 24;

  // Shading across the *thickness* of the ring is what makes a slice read as
  // glass. Translucency alone does not: over a pure-black page there is nothing
  // behind the arc to show through, so a lowered opacity just looks like a
  // darker flat fill, and under saturated colour it disappears entirely.
  //
  // The shading has to be one continuous gradient, not concentric strokes at
  // stepped opacities — those band, and the ring ends up looking like three
  // flat rings stacked rather than one curved surface. A radial gradient in
  // user space varies with distance from the centre, which for a ring *is* the
  // cross-section direction, so a single overlay arc painted with it shades the
  // slice smoothly from a dark inner edge to a lit outer edge.
  //
  // Stop offsets are fractions of OUTER_EDGE, so they read as radii: the ring
  // spans RADIUS ± SLICE_WIDTH / 2, i.e. 0.74–1.0 of the gradient.
  const OUTER_EDGE = RADIUS + SLICE_WIDTH / 2;
  const TUBE_STOPS = [
    { radius: RADIUS - SLICE_WIDTH / 2, color: "#000000", opacity: 0.42 },
    { radius: 73, color: "#000000", opacity: 0.16 },
    { radius: 79, color: "#000000", opacity: 0.02 },
    { radius: 85, color: "#ffffff", opacity: 0.1 },
    { radius: 90.5, color: "#ffffff", opacity: 0.22 },
    { radius: OUTER_EDGE, color: "#ffffff", opacity: 0.06 },
  ];
  // Surface showing between neighbouring slices (viewBox units), so identity
  // never rests on the color boundary alone.
  const SLICE_GAP = 2;

  // The three constants the arc math in `$lib/breakdown` needs, bundled once
  // so no call site can pass a mismatched set.
  const DONUT = { circumference: CIRCUMFERENCE, sliceGap: SLICE_GAP, palette: PALETTE };

  // The comparison period's ring, concentric inside the main one. It sits far
  // enough in to leave a clear band of background between the two — touching,
  // they would read as one thick ring with a seam rather than as two.
  //
  // Two rings can never be read arc-against-arc: a slice's start angle depends
  // on every slice before it, so the same category begins somewhere different
  // in each ring and nothing lines up. That limit is inherent to the shape, and
  // the fix is the hover the page already has — pointing at a category lights
  // it up in both rings, the legend and the list at once, which turns "compare
  // everything at a glance" into "compare any one instantly".
  //
  // What the rings *can* show at a glance is the shape of each period: whether
  // spending was spread evenly or dominated by one category, and whether that
  // changed. That is what the donut is for, and it is the one reading the
  // paired bars below don't give.
  const COMPARE_RADIUS = 54;

  // Thickness carries the period's total, so a month that cost more is visibly
  // fatter — the magnitude a share-based chart otherwise throws away entirely.
  //
  // Clamped hard, and deliberately narrow. Width is a weak perceptual channel,
  // the exact figures are already printed in the middle of the ring, and an
  // unclamped ratio would turn a quiet month into a hairline and a bad one into
  // a band thicker than the ring it is nested in. It is a nudge, not a
  // measurement.
  const COMPARE_WIDTH = 11;
  const COMPARE_WIDTH_MIN = 7;
  const COMPARE_WIDTH_MAX = 15;

  function compareRingWidth(totalA: number, totalB: number): number {
    return breakdown.compareRingWidth(
      totalA,
      totalB,
      COMPARE_WIDTH,
      COMPARE_WIDTH_MIN,
      COMPARE_WIDTH_MAX,
    );
  }

  function compareArcs<T extends { percentB: number }>(rows: T[]) {
    return breakdown.compareArcs(rows, COMPARE_RADIUS, SLICE_GAP, fillProgress);
  }

  let categories = $state<CategoryDto[]>([]);
  let transactions = $state<TransactionDto[]>([]);
  // The comparison period's rows. Kept separate rather than merged into
  // `transactions` with a tag: everything downstream of the primary period —
  // the donuts, the totals, the hidden-category bookkeeping — is about period
  // A alone, and a merged list would have to be re-split at every one of them.
  let transactionsB = $state<TransactionDto[]>([]);
  let loading = $state(true);
  let error = $state("");

  // Everything on this page the user set by hand, kept across navigation —
  // see `$lib/pageCache`. The transactions themselves are not cached; they're
  // re-fetched on every mount as usual.
  const view = pageViewState("details", () => ({
    rangeMode: "month" as RangeMode,
    rangeOffset: 0,
    comparing: false,
    compareOffset: -1,
    customStart: oneMonthAgoIsoDate(),
    customEnd: todayIsoDate(),
    expanded: {
      expense: new Set<string>(),
      income: new Set<string>(),
    } as Record<PanelKey, Set<string>>,
    hidden: {
      expense: new Set<string>(),
      income: new Set<string>(),
    } as Record<PanelKey, Set<string>>,
  }));

  let rangeMode = $state<RangeMode>(view.rangeMode);
  // Whole periods back from the one containing today: -1 is last month, -2 the
  // one before. Only `month` and `year` can be stepped — "All Time" has one
  // period and "Set Dates" is already an explicit answer.
  let rangeOffset = $state(view.rangeOffset);
  let customStart = $state(view.customStart);
  let customEnd = $state(view.customEnd);

  let steppable = $derived(rangeMode === "month" || rangeMode === "year");

  // Whether the page is showing two periods side by side, and which period the
  // second one is. `compareOffset` is absolute like `rangeOffset`, not a gap:
  // it is what the second stepper edits directly. Stepping the *first* period
  // moves both together (see `stepPeriod`), which is what keeps a comparison
  // the user set up as "against the month before" saying that at every month
  // they walk back to.
  let comparing = $state(view.comparing);
  let compareOffset = $state(view.compareOffset);

  // "All Time" is one period — there is no second one to hold it against.
  // Every other mode has a defensible predecessor: the previous month or year,
  // or for hand-picked dates the equally-long span ending just before them.
  let canCompare = $derived(rangeMode !== "all");
  let compareActive = $derived(comparing && canCompare);

  /** The second period's bounds. In `custom` mode it trails period A by its
   * own length rather than being independently steppable — there is no
   * calendar unit to step, and matching the length is what keeps the two
   * halves of the comparison the same size. */
  function computeRangeB(): { start: string; end: string } {
    if (rangeMode === "custom") return precedingSpan(customStart, customEnd);
    return computeRange(rangeMode, { offset: compareOffset });
  }

  // Hovering the donut slice, the legend entry, or the top-level breakdown
  // row for a category highlights all three — one shared id drives every
  // view of "this category" at once. Subcategory rows aren't included: they
  // have no slice or legend entry of their own to light up.
  let hoveredCategoryId = $state<string | null>(null);

  type PanelKey = "expense" | "income";

  // Which root categories are expanded, per panel — an expanded row shows
  // its subcategories as extra rows underneath it, inline in the same list,
  // rather than replacing the panel's own root-level breakdown.
  let expandedCategoryIds = $state<Record<PanelKey, Set<string>>>({
    expense: new Set(view.expanded.expense),
    income: new Set(view.expanded.income),
  });

  // Categories the user has hidden via the eye toggle, per panel. Hiding is
  // per-panel because the toggle lives on a panel's own breakdown row: hiding
  // a category from Expenses shouldn't silently reshape the Income donut.
  // A hidden category's transactions drop out of that panel's total, so every
  // other category's percentage recomputes over what's left.
  let hiddenCategoryIds = $state<Record<PanelKey, Set<string>>>({
    expense: new Set(view.hidden.expense),
    income: new Set(view.hidden.income),
  });

  // Mirrors the user's choices back into the cache. Copies rather than storing
  // the `$state` proxies themselves, so what outlives this component is a
  // plain object and not a handle into a torn-down reactive graph.
  $effect(() => {
    view.rangeMode = rangeMode;
    view.rangeOffset = rangeOffset;
    view.comparing = comparing;
    view.compareOffset = compareOffset;
    view.customStart = customStart;
    view.customEnd = customEnd;
    view.expanded = {
      expense: new Set(expandedCategoryIds.expense),
      income: new Set(expandedCategoryIds.income),
    };
    view.hidden = {
      expense: new Set(hiddenCategoryIds.expense),
      income: new Set(hiddenCategoryIds.income),
    };
  });

  /** A change in money, always carrying its sign — "+€62,10", "−€45,00".
   * `formatCurrency` renders a negative amount with a leading "-", but a
   * *positive* delta needs its "+" said out loud too: without it "€62,10"
   * beside "−€45,00" reads as an amount next to a change rather than as two
   * changes. */
  function formatDelta(minorUnits: number, code: string): string {
    if (minorUnits === 0) return `±${formatCurrency(0, code)}`;
    const sign = minorUnits > 0 ? "+" : "−";
    return `${sign}${formatCurrency(Math.abs(minorUnits), code)}`;
  }

  /** Whether a change is good news for the user, which is the opposite thing
   * in the two panels: spending more is bad, earning more is good. Colouring
   * both by the sign of the number would paint every raise red.
   *
   * Returns null for no change and for the expense/income *totals* being
   * genuinely neutral, so nothing gets a verdict it hasn't earned. */
  function favourability(panel: PanelKey, deltaMinor: number): "good" | "bad" | null {
    if (deltaMinor === 0) return null;
    const grew = deltaMinor > 0;
    return panel === "expense" ? (grew ? "bad" : "good") : grew ? "good" : "bad";
  }

  /** The relative change as text, or empty when there is no honest percentage
   * to give: a category with nothing in one of the two periods has no ratio,
   * only a division by zero or by a number that leaves 100%.
   *
   * Empty rather than the words "new" and "gone". The row already says it and
   * says it better — a €0,00 beside a €600,00, with a bar on one line and
   * nothing on the other, is unmistakable. The label was restating the two
   * figures either side of it. */
  function formatRatio(row: {
    amountMinorUnits: number;
    amountB: number;
    deltaRatio: number | null;
  }): string {
    if (row.deltaRatio === null || row.amountMinorUnits === 0) return "";
    const pct = row.deltaRatio * 100;
    const sign = pct > 0 ? "+" : pct < 0 ? "−" : "";
    return `${sign}${Math.abs(pct).toFixed(pct !== 0 && Math.abs(pct) < 10 ? 1 : 0)}%`;
  }

  function categoryHasChildren(id: string): boolean {
    return categories.some((c) => c.parent_id === id);
  }

  // A category is only worth expanding if doing so would reveal a genuinely
  // different breakdown — i.e. at least one of its transactions is actually
  // assigned to a child category. A category with subcategories defined but
  // whose transactions are all logged directly against it (e.g. Transportation
  // with no transaction ever assigned to a specific subcategory) would just
  // show itself again under itself, which isn't useful.
  function hasVisibleSubcategories(
    txns: TransactionDto[],
    txnsB: TransactionDto[],
    rootId: string,
  ): boolean {
    if (!categoryHasChildren(rootId)) return false;
    const hasChildRow = (list: TransactionDto[]) =>
      list.some((t) => t.category_id !== rootId && rootCategoryId(t.category_id) === rootId);
    // Either period is enough. A category that was broken down by subcategory
    // in June and lumped together in August still has a breakdown worth
    // opening — that shift is itself part of what changed.
    return hasChildRow(txns) || hasChildRow(txnsB);
  }

  function toggleExpand(panel: PanelKey, categoryId: string) {
    const next = new Set(expandedCategoryIds[panel]);
    if (next.has(categoryId)) next.delete(categoryId);
    else next.add(categoryId);
    expandedCategoryIds = { ...expandedCategoryIds, [panel]: next };
  }

  // Animates the donut/bars filling up from empty whenever the breakdown
  // they're drawn from changes (initial load, range/tab switch, category
  // filter toggle) rather than snapping straight to the final shape.
  let fillProgress = $state(0);
  let fillFrame: number | undefined;

  function animateFill() {
    if (fillFrame !== undefined) cancelAnimationFrame(fillFrame);
    const duration = 700;
    const start = performance.now();
    fillProgress = 0;
    function tick(now: number) {
      const t = Math.min((now - start) / duration, 1);
      fillProgress = 1 - Math.pow(1 - t, 3);
      fillFrame = t < 1 ? requestAnimationFrame(tick) : undefined;
    }
    fillFrame = requestAnimationFrame(tick);
  }

  onMount(load);
  onMount(() => {
    return () => {
      if (fillFrame !== undefined) cancelAnimationFrame(fillFrame);
    };
  });

  // On `window` rather than an element, so the shortcut works wherever the
  // user's focus happens to be on the page — and torn down with the component,
  // since it steps *this* page's period and means nothing anywhere else.
  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  });

  async function load() {
    loading = true;
    error = "";
    try {
      const range = computeRange(rangeMode, {
        start: customStart,
        end: customEnd,
        offset: rangeOffset,
      });
      // The comparison period is a second query over the same command rather
      // than one widened query split afterwards: the two periods needn't be
      // adjacent (August against June leaves July in between), so a single
      // range covering both would fetch a month nothing on the page shows.
      const rangeB = compareActive ? computeRangeB() : null;
      const [c, t, tb] = await Promise.all([
        api.listCategories(),
        api.listTransactions(range.start, range.end),
        rangeB ? api.listTransactions(rangeB.start, rangeB.end) : Promise.resolve([]),
      ]);
      categories = c;
      transactions = t;
      transactionsB = tb;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  /** Collapsing on a range change is deliberate: the rows underneath an
   * expanded category are about to be a different breakdown entirely. It
   * lives here rather than in `load()` so that remounting the page — which
   * also calls `load()` — restores what the user had open instead of
   * closing it behind their back. */
  function setRange(mode: RangeMode) {
    rangeMode = mode;
    // A month offset means nothing as a year offset — two months back is not
    // two years back — so switching mode lands on the current period rather
    // than silently reinterpreting the step. The comparison gap is reset for
    // the same reason: carried across, a three-months-back comparison would
    // reappear as three *years* back.
    rangeOffset = 0;
    compareOffset = -1;
    expandedCategoryIds = { expense: new Set(), income: new Set() };
    load();
  }

  function setCustomRange(start: string, end: string) {
    customStart = start;
    customEnd = end;
    expandedCategoryIds = { expense: new Set(), income: new Set() };
    load();
  }

  /** The present is the ceiling: offset 0 is the period containing today, and
   * nothing may step past it. A future month holds no spending to look at, so
   * an arrow leading there only offers the user a series of empty pages to
   * walk back out of.
   *
   * Both periods are capped, not just the primary one — the comparison period
   * moves with it (see `stepPeriod`), so capping only the visible arrow would
   * let the second period be carried over the line by the first. */
  let canStepForward = $derived(rangeOffset < 0);
  let canStepCompareForward = $derived(compareOffset < 0);

  /** Steps the selected period, and resets what was expanded for the same
   * reason `setRange` does: the rows under an open category are about to be a
   * different month's breakdown entirely. */
  function stepPeriod(delta: number) {
    const next = Math.min(rangeOffset + delta, 0);
    if (next === rangeOffset) return;
    rangeOffset = next;
    // The comparison period travels with it, holding the gap the user set up.
    // Someone comparing August against June is asking about a two-month gap,
    // not about June specifically — walking back to July should show May, not
    // pin June and silently turn the question into a different one. It gets
    // its own ceiling rather than blocking the primary arrow: the user pressed
    // the arrow for *this* period, and refusing to move it because the other
    // one is already at the present would be answering a question they didn't
    // ask.
    compareOffset = Math.min(compareOffset + delta, 0);
    expandedCategoryIds = { expense: new Set(), income: new Set() };
    load();
  }

  function resetPeriod() {
    if (rangeOffset === 0) return;
    compareOffset = Math.min(compareOffset - rangeOffset, 0);
    rangeOffset = 0;
    expandedCategoryIds = { expense: new Set(), income: new Set() };
    load();
  }

  /** Steps the comparison period alone, which is how the gap gets set in the
   * first place. Landing on period A is allowed: the deltas all read zero,
   * which is a perfectly clear way for the view to say the two periods are the
   * same one, and skipping over it would move the period further than the
   * arrow the user pressed says it should. */
  function stepCompare(delta: number) {
    const next = Math.min(compareOffset + delta, 0);
    if (next === compareOffset) return;
    compareOffset = next;
    expandedCategoryIds = { expense: new Set(), income: new Set() };
    load();
  }

  /** ← / → step the period, matching the arrows in the range bar.
   *
   * Bare arrows only. `Cmd`/`Alt` + arrow is already the app's page-to-page
   * navigation (see `CommandPalette`), and leaving every modifier alone keeps
   * the browser's own back/forward gestures intact.
   *
   * Three things have to be true before a keypress is ours: the range mode is
   * one that *has* neighbouring periods, the user isn't typing (an arrow in a
   * text field moves the caret), and no overlay is up — with the command
   * palette open, arrows would silently walk the months behind it. */
  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;
    if (!steppable) return;
    const target = event.target;
    if (
      target instanceof HTMLElement &&
      (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)
    ) {
      return;
    }
    if (document.querySelector(".backdrop")) return;
    const delta = event.key === "ArrowLeft" ? -1 : 1;
    if (delta > 0 && !canStepForward) return;
    event.preventDefault();
    stepPeriod(delta);
  }

  function toggleCompare() {
    comparing = !comparing;
    // Reaching for Compare almost always means "against the one before".
    // Anchoring to A rather than to now keeps that true at every period the
    // user has stepped to.
    if (comparing) compareOffset = rangeOffset - 1;
    expandedCategoryIds = { expense: new Set(), income: new Set() };
    load();
  }

  let periodLabel = $derived(
    describeRange(rangeMode, rangeOffset, { start: customStart, end: customEnd }),
  );

  /** The exact days on screen, spelled out under the period label so the
   * label is never the only thing saying what "August" covers. */
  let rangeSpan = $derived.by(() => {
    if (!steppable) return "";
    const r = computeRange(rangeMode, { offset: rangeOffset });
    return formatDateSpan(r.start, r.end);
  });

  /** How much of the current period has actually happened, when it is still
   * happening. This is the guard against the partial-period trap: on the 6th,
   * "August" holds six days of spending, and set beside a whole earlier month
   * it reads as a collapse rather than as a month that has not finished. The
   * span alone can't say it — a month range is the whole calendar month
   * whether or not today is inside it — so the elapsed count is spelled out
   * separately, and only where it's true. */
  let periodProgress = $derived.by(() => {
    if (!steppable || rangeOffset !== 0) return "";
    const now = new Date();
    const r = computeRange(rangeMode, { offset: 0 });
    const dayMs = 24 * 60 * 60 * 1000;
    const startMs = new Date(`${r.start}T00:00:00`).getTime();
    const endMs = new Date(`${r.end}T00:00:00`).getTime();
    const elapsed = Math.round((now.setHours(0, 0, 0, 0) - startMs) / dayMs) + 1;
    const total = Math.round((endMs - startMs) / dayMs) + 1;
    return `${elapsed} of ${total} days so far`;
  });

  let compareLabel = $derived(
    rangeMode === "custom"
      ? "Preceding span"
      : describeRange(rangeMode, compareOffset, { start: customStart, end: customEnd }),
  );

  let compareSpan = $derived.by(() => {
    if (!compareActive) return "";
    const r = computeRangeB();
    return formatDateSpan(r.start, r.end);
  });

  /** Says so when the two periods are not the same length, which is the one
   * thing that makes every number below it unfair. It fires for the obvious
   * case — a month still in progress against a finished one — and for the
   * quieter ones nobody thinks about, like February against January.
   *
   * The comparison is still shown rather than blocked. A 28-vs-31-day month is
   * a real question people ask; it just needs saying that the answer is
   * shorter by three days. */
  let lengthMismatch = $derived.by(() => {
    if (!compareActive) return "";
    const a = computeRange(rangeMode, {
      start: customStart,
      end: customEnd,
      offset: rangeOffset,
    });
    const b = computeRangeB();
    // A period still running is measured by the days that have happened, not
    // by the days it will eventually hold — otherwise the six days of August
    // on screen would compare as a full month.
    const elapsedIfCurrent = (r: { start: string; end: string }) => {
      const today = todayIsoDate();
      return r.start <= today && today < r.end ? spanDays(r.start, today) : spanDays(r.start, r.end);
    };
    const daysA = elapsedIfCurrent(a);
    const daysB = elapsedIfCurrent(b);
    if (daysA === daysB) return "";
    return `${daysA} days vs ${daysB} — not the same length`;
  });

  /** The Transactions page, opened on the same slice of the ledger this row
   * is showing: same date range, same expense/income side, filtered to this
   * category.
   *
   * A real href rather than a `goto()` click handler — it's a navigation, so
   * it should behave like one (middle-click, Cmd-click, keyboard). The range
   * travels as the mode plus, only when it means anything, the custom
   * endpoints: handing over "month" rather than two computed dates keeps the
   * two pages agreeing about what "this month" is instead of freezing this
   * page's answer into the URL.
   *
   * That only holds while this page is *on* the current period, though. The
   * Transactions page has no offset of its own, so a stepped-back month has
   * to travel as explicit dates under `custom` — handing it "month" would
   * open August's list from under June's row. The two pages agreeing about
   * "now" was the point of passing a mode; there is no shared "now" to agree
   * about once the user has walked away from it.
   *
   * Filtering by a parent gives that parent's whole branch — the backend
   * rolls subcategories into the named category (see `TransactionFilters`),
   * matching the rollup this row's own amount is built from. Without that,
   * the row and the list it opens would disagree. */
  function transactionsHref(panel: PanelKey, categoryId: string): string {
    const stepped = steppable && rangeOffset !== 0;
    const params = new URLSearchParams({
      kind: panel,
      category: categoryId,
      range: stepped ? "custom" : rangeMode,
    });
    if (stepped) {
      const r = computeRange(rangeMode, { offset: rangeOffset });
      params.set("start", r.start);
      params.set("end", r.end);
    } else if (rangeMode === "custom") {
      params.set("start", customStart);
      params.set("end", customEnd);
    }
    return `/transactions?${params}`;
  }

  function toggleHidden(panel: PanelKey, categoryId: string) {
    const next = new Set(hiddenCategoryIds[panel]);
    if (next.has(categoryId)) next.delete(categoryId);
    else next.add(categoryId);
    hiddenCategoryIds = { ...hiddenCategoryIds, [panel]: next };
  }

  // Hiding a root hides everything under it, so a subcategory is out whenever
  // it is hidden itself *or* its root is — otherwise unhiding a root would
  // have to re-add each of its children one by one.
  function isHidden(panel: PanelKey, categoryId: string): boolean {
    const hidden = hiddenCategoryIds[panel];
    return hidden.has(categoryId) || hidden.has(rootCategoryId(categoryId));
  }

  let rootMap = $derived.by(() => breakdown.buildRootMap(categories));

  function rootCategoryId(categoryId: string): string {
    return rootMap.get(categoryId) ?? categoryId;
  }

  function categoryName(id: string): string {
    return categories.find((c) => c.id === id)?.name ?? "Uncategorized";
  }

  /** This whole page is a breakdown of spending and income, so transfers
   * between the user's own accounts and reconciliation adjustments are
   * excluded outright — they would show up as a category slice representing
   * money that was never spent. */
  let reportableTransactions = $derived(transactions.filter(countsTowardTotals));

  let currency = $derived(transactions[0]?.currency ?? "EUR");

  // Each panel keeps both sets: the visible transactions drive the donut and
  // every percentage, while the full set is what still knows a hidden
  // category exists at all — its row has to stay in the list, or there'd be
  // no eye left to click to bring it back.
  let expenseTransactions = $derived(
    reportableTransactions.filter((t) => t.amount_minor_units < 0),
  );
  let incomeTransactions = $derived(
    reportableTransactions.filter((t) => t.amount_minor_units > 0),
  );

  let visibleExpenseTransactions = $derived(
    expenseTransactions.filter((t) => !isHidden("expense", t.category_id)),
  );
  let visibleIncomeTransactions = $derived(
    incomeTransactions.filter((t) => !isHidden("income", t.category_id)),
  );

  // The comparison period goes through exactly the same sieve as period A —
  // transfers and adjustments out, sign split, hidden categories dropped. A
  // category hidden from Expenses has to leave both periods' totals or the two
  // percentages beside each other would be shares of different denominators.
  let reportableTransactionsB = $derived(transactionsB.filter(countsTowardTotals));
  let visibleExpenseTransactionsB = $derived(
    reportableTransactionsB.filter(
      (t) => t.amount_minor_units < 0 && !isHidden("expense", t.category_id),
    ),
  );
  let visibleIncomeTransactionsB = $derived(
    reportableTransactionsB.filter(
      (t) => t.amount_minor_units > 0 && !isHidden("income", t.category_id),
    ),
  );

  let netLeftMinorUnits = $derived(
    [...visibleExpenseTransactions, ...visibleIncomeTransactions].reduce(
      (sum, t) => sum + t.amount_minor_units,
      0,
    ),
  );

  let netLeftMinorUnitsB = $derived(
    [...visibleExpenseTransactionsB, ...visibleIncomeTransactionsB].reduce(
      (sum, t) => sum + t.amount_minor_units,
      0,
    ),
  );

  function buildBreakdown(txns: TransactionDto[], scopeRootId: string | null) {
    return breakdown.buildBreakdown(txns, scopeRootId, rootCategoryId, categoryName);
  }

  function hiddenRows(panel: PanelKey, txns: TransactionDto[], scopeRootId: string | null) {
    return breakdown.hiddenRows(
      txns,
      hiddenCategoryIds[panel],
      scopeRootId,
      rootCategoryId,
      categoryName,
    );
  }

  type PanelRow = breakdown.PanelRow;

  function buildPanelRows(
    txnsA: TransactionDto[],
    txnsB: TransactionDto[],
    scopeRootId: string | null,
  ) {
    return breakdown.buildPanelRows(
      txnsA,
      txnsB,
      scopeRootId,
      compareActive,
      rootCategoryId,
      categoryName,
      DONUT,
    );
  }

  // Colored (but non-animated) breakdown of one root category's subcategories,
  // for rendering the expanded rows nested under it in the breakdown list.
  // Each row gets two percentages: `percent` is share of the parent category
  // (e.g. Rent's share of Housing), `percentOfTotal` is share of the whole
  // panel (e.g. Rent's share of all Expenses) — the two answer different
  // questions and both are useful side by side.
  function subCategoryBreakdown(
    txns: TransactionDto[],
    txnsB: TransactionDto[],
    rootId: string,
    panelTotal: number,
  ) {
    const panelTotalOrOne = panelTotal || 1;
    return buildPanelRows(txns, txnsB, rootId).rows.map((slice) => ({
      ...slice,
      percentOfTotal: (slice.amountMinorUnits / panelTotalOrOne) * 100,
    }));
  }

  function withAnimatedSlices<T extends { percent: number; dashoffset: number }>(slices: T[]) {
    return breakdown.withAnimatedSlices(slices, DONUT, fillProgress);
  }

  let expenseData = $derived.by(() =>
    buildPanelRows(visibleExpenseTransactions, visibleExpenseTransactionsB, null),
  );
  let incomeData = $derived.by(() =>
    buildPanelRows(visibleIncomeTransactions, visibleIncomeTransactionsB, null),
  );

  let animatedExpenseSlices = $derived(withAnimatedSlices(expenseData.rows));
  let animatedIncomeSlices = $derived(withAnimatedSlices(incomeData.rows));

  let expenseHiddenRows = $derived(hiddenRows("expense", expenseTransactions, null));
  let incomeHiddenRows = $derived(hiddenRows("income", incomeTransactions, null));

  $effect(() => {
    expenseData;
    incomeData;
    if (!loading) animateFill();
  });
</script>

<h1>Details</h1>

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
    <DateRangePicker start={customStart} end={customEnd} onChange={setCustomRange} />
  {:else if steppable}
    <!-- The label is itself the way back to the current period. It carries
         the accent only when there is somewhere to go back to, so a period
         that isn't "now" is visibly not "now" and the affordance appears
         exactly when it does something. -->
    <div class="period-nav">
      <button
        type="button"
        class="nav-button"
        onclick={() => stepPeriod(-1)}
        aria-label={`Previous ${rangeMode}`}
        title={`Previous ${rangeMode} (←)`}
      >
        <ChevronLeft size={16} />
      </button>
      <button
        type="button"
        class="period-label"
        class:stepped={rangeOffset !== 0}
        disabled={rangeOffset === 0}
        onclick={resetPeriod}
        aria-label={rangeOffset === 0
          ? `Showing ${periodLabel}`
          : `Showing ${periodLabel} — back to this ${rangeMode}`}
        title={rangeOffset === 0 ? undefined : `Back to this ${rangeMode}`}
      >
        <span class="period-name">{periodLabel}</span>
        <span class="period-span">{rangeSpan}</span>
      </button>
      <!-- Disabled rather than hidden at the present: a control that vanishes
           at the end of its range leaves the label sliding sideways under the
           pointer, and says nothing about why. -->
      <button
        type="button"
        class="nav-button"
        disabled={!canStepForward}
        onclick={() => stepPeriod(1)}
        aria-label={`Next ${rangeMode}`}
        title={canStepForward
          ? `Next ${rangeMode} (→)`
          : `${rangeMode === "month" ? "This month" : "This year"} is the latest there is`}
      >
        <ChevronRight size={16} />
      </button>
    </div>
    <!-- Outside the stepper, not under the label with the dates: as part of
         the button it was the widest thing in it, so the arrows jumped
         sideways every time the user stepped off the current period and the
         note disappeared. It is a remark about what's on screen rather than
         part of the control. -->
    {#if periodProgress && !compareActive}
      <span class="period-progress">{periodProgress}</span>
    {/if}
  {/if}

  {#if compareActive}
    <span class="vs">vs</span>
    <div class="period-nav compare">
      {#if steppable}
        <button
          type="button"
          class="nav-button"
          onclick={() => stepCompare(-1)}
          aria-label={`Previous comparison ${rangeMode}`}
          title={`Previous comparison ${rangeMode}`}
        >
          <ChevronLeft size={16} />
        </button>
      {/if}
      <div class="period-label static">
        <span class="period-name">{compareLabel}</span>
        <span class="period-span">{compareSpan}</span>
      </div>
      {#if steppable}
        <button
          type="button"
          class="nav-button"
          disabled={!canStepCompareForward}
          onclick={() => stepCompare(1)}
          aria-label={`Next comparison ${rangeMode}`}
          title={canStepCompareForward
            ? `Next comparison ${rangeMode}`
            : `${rangeMode === "month" ? "This month" : "This year"} is the latest there is`}
        >
          <ChevronRight size={16} />
        </button>
      {/if}
    </div>
  {/if}

  <button
    type="button"
    class="compare-btn"
    class:active={compareActive}
    disabled={!canCompare}
    aria-pressed={compareActive}
    onclick={toggleCompare}
    title={canCompare
      ? "Compare this period against another"
      : "All Time is a single period — there is nothing to compare it against"}
  >
    <GitCompareArrows size={14} aria-hidden="true" />
    Compare
  </button>
</div>

<!-- Loud enough to be read before the numbers are, because it is the thing
     that decides whether they mean anything. -->
{#if lengthMismatch}
  <p class="mismatch">
    <TriangleAlert size={14} aria-hidden="true" />
    {lengthMismatch}
  </p>
{/if}

<!-- Sits in the row's trailing action column beside the eye, not inside
     `.row-main` — on a parent row that whole area is already the
     expand/collapse button, and a button nested in a button is invalid.
     Borrows the eye's low-opacity-until-pointed-at treatment so the two read
     as one family of row actions rather than as competing affordances. -->
{#snippet goToLink(panel: PanelKey, categoryId: string, name: string)}
  <a
    class="goto-btn"
    href={transactionsHref(panel, categoryId)}
    title={`View ${name} transactions`}
    aria-label={`View ${name} transactions`}
  >
    <ArrowUpRight size={15} aria-hidden="true" />
  </a>
{/snippet}

{#snippet eyeToggle(panel: PanelKey, categoryId: string, name: string, hidden: boolean)}
  <button
    type="button"
    class="eye-btn"
    class:is-hidden={hidden}
    aria-pressed={hidden}
    title={hidden ? `Show ${name}` : `Hide ${name}`}
    aria-label={hidden ? `Show ${name}` : `Hide ${name}`}
    onclick={() => toggleHidden(panel, categoryId)}
  >
    <svg class="eye" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M2 12s3.8-6.6 10-6.6 10 6.6 10 6.6-3.8 6.6-10 6.6S2 12 2 12Z" />
      <circle cx="12" cy="12" r="3.1" />
      {#if hidden}<line x1="4" y1="20" x2="20" y2="4" />{/if}
    </svg>
  </button>
{/snippet}

<!-- The change in money, said first and loudest. A share-of-total change alone
     would be actively misleading: when the panel's own total moves, a category
     can take a *larger share* of it while costing *less*, and a row saying
     only "+3pp" would report that as a rise. The relative change rides along
     after it, muted, for the size the absolute figure can't convey. -->
{#snippet deltaChip(
  panel: PanelKey,
  row: { amountMinorUnits: number; amountB: number; deltaMinor: number; deltaRatio: number | null },
)}
  <span class="delta {favourability(panel, row.deltaMinor) ?? 'flat'}">
    {#if row.deltaMinor === 0}
      <!-- "= ±€0,00 0%" is three tokens agreeing that nothing happened. One
           says it. -->
      <span class="delta-flat-text">no change</span>
    {:else}
      <!-- The arrow says up or down on its own, so the good/bad colouring is a
           second reading of the same fact rather than the only one — the
           difference survives being printed, and being colour-blind. -->
      <span class="delta-arrow" aria-hidden="true">{row.deltaMinor > 0 ? "▲" : "▼"}</span>
      <span class="delta-amount">{formatDelta(row.deltaMinor, currency)}</span>
      {@const ratio = formatRatio(row)}
      {#if ratio}<span class="delta-ratio">{ratio}</span>{/if}
    {/if}
  </span>
{/snippet}

<!-- The comparison period's own figures, under the current period's rather
     than beside them. Side by side as "X% vs Y%" the two would compete for
     which one is *the* number; stacked and muted, the top line still reads as
     the answer and the one below as what it is being measured against.

     The delta rides at the end of this line rather than on the one above.
     Sharing a line with the name it left too little room for the name itself,
     which ellipsised down to "S…" and "Dinin…" — and the name is the one thing
     in the row nothing else can stand in for. Down here it also sits directly
     beside the two figures it is the difference of, and the top line stays
     byte-identical to the non-comparing view, so turning Compare on adds a
     line rather than reflowing the one that was already there. It keeps colour
     and weight against a muted row, so it still reads first. -->
{#snippet priorLine(
  panel: PanelKey,
  panelLabel: string,
  row: {
    amountMinorUnits: number;
    amountB: number;
    percentB: number;
    deltaMinor: number;
    deltaRatio: number | null;
  },
)}
  <div class="prior-row" title={`${compareLabel} — share of ${panelLabel.toLowerCase()}`}>
    <!-- The period's name was printed on every row of both panels, which on a
         page with a dozen categories meant reading "July 2026" twenty times to
         learn it once. The range bar at the top already names both periods,
         and the striped bar under this line pairs it to the right one. What
         sighted users lose is repetition; what a screen reader would lose is
         the only clue, so the name stays here for them. -->
    <span class="visually-hidden">{compareLabel}</span>
    <span class="prior-spacer"></span>
    <span class="amount">{formatCurrency(row.amountB, currency)}</span>
    <span class="percent">{row.percentB.toFixed(1)}%</span>
    {@render deltaChip(panel, row)}
  </div>
{/snippet}

<!-- Two lanes on one scale: the current period solid on top, the comparison
     period striped below it, both in the category's own colour.

     This replaced a white tick marking where the comparison period's share
     fell on the single bar. The tick was accurate but had to be decoded — it
     was a mark in a colour belonging to nothing, and which of the two periods
     it stood for was not on screen anywhere. Two bars need no decoding: the
     longer one is bigger, and they share a left edge and a scale, so the
     difference is the overhang. Striping rather than a second hue keeps the
     category's identity intact — the pair still reads as one category — while
     saying plainly which of the two is the one being measured against.

     The stripes are a gradient over the colour rather than a lighter tint of
     it: a tint of a dark palette slot on a black page can land close to the
     track itself, whereas the texture survives at any slot's lightness. -->
{#snippet comparedBar(bar: { color: string; solidPercent: number; priorPercent: number })}
  <div class="bar-track">
    <div class="bar-lane">
      <div
        class="bar-fill"
        style={`width:${bar.solidPercent}%;background-color:${bar.color}`}
      ></div>
    </div>
    {#if compareActive}
      <div class="bar-lane">
        <div
          class="bar-fill prior"
          style={`width:${bar.priorPercent}%;background-color:${bar.color}`}
        ></div>
      </div>
    {/if}
  </div>
{/snippet}

<!-- One object rather than a positional list: comparing doubled almost every
     argument (a total and a total-before, a period's rows and the other
     period's), and eight same-typed positional arrays is a call site nobody
     can read or safely reorder. -->
{#snippet donutPanel(p: {
  label: string;
  panelKey: PanelKey;
  total: number;
  totalB: number;
  slices: typeof animatedExpenseSlices;
  txns: TransactionDto[];
  txnsB: TransactionDto[];
  allTxns: TransactionDto[];
  allTxnsB: TransactionDto[];
  hidden: { categoryId: string; name: string; amountMinorUnits: number }[];
})}
  {@const label = p.label}
  {@const panelKey = p.panelKey}
  {@const total = p.total}
  {@const slices = p.slices}
  {@const hidden = p.hidden}
  <div class="graph-column">
    <div class="graph-graphics" class:comparing={compareActive}>
      <h2 class="panel-title">
        {label}
        <!-- The key for the inner ring, and the only place the comparison
             period is named per-panel now that the rows no longer repeat it.
             The swatch carries the same diagonal as the ring and the bars, so
             the three read as one statement about which period is which. -->
        {#if compareActive}
          <span class="panel-subtitle">
            <span class="stripe-swatch" aria-hidden="true"></span>
            {compareLabel} · {formatCurrency(p.totalB, currency)}
          </span>
        {/if}
      </h2>
      <div class="donut-wrap">
        <svg viewBox="0 0 200 200" class="donut">
          <defs>
            <!-- ids are per-panel: expense and income donuts coexist in one document -->
            <radialGradient
              id={`donut-tube-${panelKey}`}
              gradientUnits="userSpaceOnUse"
              cx="100"
              cy="100"
              r={OUTER_EDGE}
            >
              {#each TUBE_STOPS as stop (stop.radius)}
                <stop
                  offset={stop.radius / OUTER_EDGE}
                  stop-color={stop.color}
                  stop-opacity={stop.opacity}
                />
              {/each}
            </radialGradient>
            <linearGradient id={`donut-sheen-${panelKey}`} x1="0.05" y1="0" x2="0.75" y2="1">
              <stop offset="0%" stop-color="#ffffff" stop-opacity="0.18" />
              <stop offset="18%" stop-color="#ffffff" stop-opacity="0.08" />
              <stop offset="42%" stop-color="#ffffff" stop-opacity="0.015" />
              <stop offset="100%" stop-color="#ffffff" stop-opacity="0" />
            </linearGradient>
            <!-- The same diagonal cut the comparison bars use, so "striped"
                 means the earlier period everywhere on the page. Painted as
                 transparent-and-black over the colour rather than as two
                 colours, so one pattern serves all eight palette slots. -->
            <pattern
              id={`donut-stripes-${panelKey}`}
              width="6"
              height="6"
              patternUnits="userSpaceOnUse"
              patternTransform="rotate(45 0 0)"
            >
              <rect x="0" y="0" width="3" height="6" fill="#000000" fill-opacity="0.55" />
            </pattern>
          </defs>
          <g transform="rotate(-90 100 100)">
            <circle
              cx="100"
              cy="100"
              r={RADIUS}
              fill="none"
              stroke="var(--donut-track)"
              stroke-width={SLICE_WIDTH}
            />
            {#each slices as slice (slice.categoryId)}
              {@const dasharray = `${slice.animatedLength} ${CIRCUMFERENCE - slice.animatedLength}`}
              <!-- body + tube shading share one group, so the group opacity
                   composites them as a single translucent pane rather than
                   each layer fading independently -->
              <g
                class="slice"
                class:dimmed={hoveredCategoryId !== null && hoveredCategoryId !== slice.categoryId}
                role="presentation"
                onmouseenter={() => (hoveredCategoryId = slice.categoryId)}
                onmouseleave={() => (hoveredCategoryId = null)}
              >
                <circle
                  cx="100"
                  cy="100"
                  r={RADIUS}
                  fill="none"
                  stroke={slice.color}
                  stroke-width={SLICE_WIDTH}
                  stroke-dasharray={dasharray}
                  stroke-dashoffset={slice.animatedDashoffset}
                />
                <circle
                  cx="100"
                  cy="100"
                  r={RADIUS}
                  fill="none"
                  stroke={`url(#donut-tube-${panelKey})`}
                  stroke-width={SLICE_WIDTH}
                  stroke-dasharray={dasharray}
                  stroke-dashoffset={slice.animatedDashoffset}
                />
              </g>
            {/each}

            {#if compareActive}
              {@const ringWidth = compareRingWidth(total, p.totalB)}
              <circle
                cx="100"
                cy="100"
                r={COMPARE_RADIUS}
                fill="none"
                stroke="var(--donut-track)"
                stroke-width={ringWidth}
              />
              {#each compareArcs(slices) as arc (arc.categoryId)}
                <g
                  class="slice compare-slice"
                  class:dimmed={hoveredCategoryId !== null && hoveredCategoryId !== arc.categoryId}
                  role="presentation"
                  onmouseenter={() => (hoveredCategoryId = arc.categoryId)}
                  onmouseleave={() => (hoveredCategoryId = null)}
                >
                  <circle
                    cx="100"
                    cy="100"
                    r={COMPARE_RADIUS}
                    fill="none"
                    stroke={arc.color}
                    stroke-width={ringWidth}
                    stroke-dasharray={arc.arcDasharray}
                    stroke-dashoffset={arc.arcDashoffset}
                  />
                  <!-- No tube gradient on this ring. Its stops are radii
                       measured against OUTER_EDGE, so at this radius they
                       land deep in the dark inner end and would black the
                       arc out — and a flat inner ring is the right hierarchy
                       anyway: the glass belongs to the period in focus. -->
                  <circle
                    cx="100"
                    cy="100"
                    r={COMPARE_RADIUS}
                    fill="none"
                    stroke={`url(#donut-stripes-${panelKey})`}
                    stroke-width={ringWidth}
                    stroke-dasharray={arc.arcDasharray}
                    stroke-dashoffset={arc.arcDashoffset}
                  />
                </g>
              {/each}
            {/if}
          </g>
          <!-- Fixed light direction: the sheen sits outside the rotated group.
               It covers the whole ring, so without pointer-events="none" it
               swallows every hover meant for the slices underneath it. -->
          <circle
            cx="100"
            cy="100"
            r={RADIUS}
            fill="none"
            stroke={`url(#donut-sheen-${panelKey})`}
            stroke-width={SLICE_WIDTH}
            pointer-events="none"
            role="presentation"
          />
        </svg>
        <!-- The comparison ring takes the middle in from radius 68 to 47, and
             the two extra lines go in at the same time — so the type steps
             down to keep the block inside the hole rather than colliding with
             the ring around it. -->
        <div class="donut-center" class:compact={compareActive}>
          <span class="total">{formatCurrency(total, currency)}</span>
          <span class="label">{label}</span>
          {#if compareActive}
            {@const delta = total - p.totalB}
            <span class="center-delta {favourability(panelKey, delta) ?? 'flat'}">
              {formatDelta(delta, currency)}
            </span>
          {/if}
        </div>
      </div>

      <!-- Rendered even when empty: it holds the column that keeps this panel's
           donut horizontally aligned with the other panel's. -->
      <!-- Only categories with a slice to point at. A comparison row for a
           category that has nothing in this period would be a colour swatch
           beside a name with no matching arc anywhere on the ring. -->
      <ul class="legend">
        {#each slices.filter((s) => s.amountMinorUnits > 0) as slice (slice.categoryId)}
          <li
            class:dimmed={hoveredCategoryId !== null && hoveredCategoryId !== slice.categoryId}
            onmouseenter={() => (hoveredCategoryId = slice.categoryId)}
            onmouseleave={() => (hoveredCategoryId = null)}
          >
            <span class="dot" style={`background-color:${slice.color}`}></span>
            <span class="name">{slice.name}</span>
          </li>
        {/each}
      </ul>
    </div>

    {#if slices.length === 0 && hidden.length === 0}
      <p class="empty">No {label.toLowerCase()} in this range.</p>
    {:else}
      <ul class="breakdown">
        {#each slices as slice (slice.categoryId)}
          {@const hasChildren = hasVisibleSubcategories(
            p.allTxns,
            p.allTxnsB,
            slice.categoryId,
          )}
          {@const expanded = expandedCategoryIds[panelKey].has(slice.categoryId)}
          {@const subHidden = hiddenRows(panelKey, p.allTxns, slice.categoryId)}
          <li
            class:dimmed={hoveredCategoryId !== null && hoveredCategoryId !== slice.categoryId}
            onmouseenter={() => (hoveredCategoryId = slice.categoryId)}
            onmouseleave={() => (hoveredCategoryId = null)}
          >
            <div class="breakdown-row">
              <button
                type="button"
                class="row-main"
                class:clickable={hasChildren}
                disabled={!hasChildren}
                onclick={() => toggleExpand(panelKey, slice.categoryId)}
              >
                <div class="row">
                  <span class="chevron" class:expanded class:invisible={!hasChildren}>›</span>
                  <span class="dot" style={`background-color:${slice.color}`}></span>
                  <span class="name">{slice.name}</span>
                  <span class="amount">{formatCurrency(slice.amountMinorUnits, currency)}</span>
                  <span class="percent">{slice.percent.toFixed(1)}%</span>
                </div>
                <!-- Both lanes sweep in together on the same `fillProgress`,
                     so the pair grows as one bar rather than the comparison
                     appearing under an already-drawn bar. -->
                {@render comparedBar({
                  color: slice.color,
                  solidPercent: slice.animatedPercent,
                  priorPercent: Math.min(slice.percentB, 100) * fillProgress,
                })}
                {#if compareActive}
                  {@render priorLine(panelKey, p.label, slice)}
                {/if}
              </button>
              {@render goToLink(panelKey, slice.categoryId, slice.name)}
              {@render eyeToggle(panelKey, slice.categoryId, slice.name, false)}
            </div>

            {#if hasChildren && expanded}
              <ul class="sub-breakdown">
                {#each subCategoryBreakdown(p.txns, p.txnsB, slice.categoryId, total) as sub (sub.categoryId)}
                  <li class="sub-row">
                    <div class="breakdown-row">
                      <div class="row-main">
                        <div class="row">
                          <span class="dot" style={`background-color:${sub.color}`}></span>
                          <span class="name">{sub.name}</span>
                          <span class="amount"
                            >{formatCurrency(sub.amountMinorUnits, currency)}</span
                          >
                          <!-- Two denominators sit side by side: the bold one
                               is this row's share of its parent (what the bar
                               below draws), the muted one its share of the
                               whole panel (what the parent row's own percent
                               means). Titles carry the wording so the row
                               stays one line. -->
                          <span class="percent" title={`Share of ${slice.name}`}
                            >{sub.percent.toFixed(1)}%</span
                          >
                          <span
                            class="percent-of-total"
                            title={`Share of total ${label.toLowerCase()}`}
                            >· {sub.percentOfTotal.toFixed(1)}% of total</span
                          >
                        </div>
                        <!-- Expanded rows appear already-drawn rather than
                             animating: they open under a bar that has long
                             since finished sweeping in. -->
                        {@render comparedBar({
                          color: sub.color,
                          solidPercent: sub.percent,
                          priorPercent: Math.min(sub.percentB, 100),
                        })}
                        {#if compareActive}
                          {@render priorLine(panelKey, slice.name, sub)}
                        {/if}
                      </div>
                      <!-- A parent's transactions logged directly against it
                           (rather than a child) surface as a sub-row carrying
                           the parent's own id — so both actions here would be
                           the parent's: an eye hiding the whole branch from
                           inside itself, and a link opening the whole branch
                           under a row showing only the directly-filed part.
                           The parent row above already owns both; this row
                           gets spacers to stay aligned. -->
                      {#if sub.categoryId === slice.categoryId}
                        <span class="action-spacer"></span>
                        <span class="action-spacer"></span>
                      {:else}
                        {@render goToLink(panelKey, sub.categoryId, sub.name)}
                        {@render eyeToggle(panelKey, sub.categoryId, sub.name, false)}
                      {/if}
                    </div>
                  </li>
                {/each}

                {#each subHidden as sub (sub.categoryId)}
                  <li class="sub-row hidden-cat">
                    <div class="breakdown-row">
                      <div class="row-main">
                        <div class="row">
                          <span class="dot muted"></span>
                          <span class="name">{sub.name}</span>
                          <span class="amount"
                            >{formatCurrency(sub.amountMinorUnits, currency)}</span
                          >
                          <span class="percent">—</span>
                        </div>
                      </div>
                      <span class="action-spacer"></span>
                      {@render eyeToggle(panelKey, sub.categoryId, sub.name, true)}
                    </div>
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}

        <!-- Hidden categories sink to the bottom of the list: they're out of
             the ranking, and keeping them in place would leave gaps in what
             reads as a sorted top-to-bottom breakdown. -->
        {#each hidden as row (row.categoryId)}
          <li class="hidden-cat">
            <div class="breakdown-row">
              <div class="row-main">
                <div class="row">
                  <span class="chevron invisible">›</span>
                  <span class="dot muted"></span>
                  <span class="name">{row.name}</span>
                  <span class="amount">{formatCurrency(row.amountMinorUnits, currency)}</span>
                  <span class="percent">—</span>
                </div>
              </div>
              <!-- No link on a hidden row: the user has excluded it from this
                   view, so the eye that brings it back is the only action it
                   should offer. The spacer keeps the eye column aligned with
                   the rows above, which do carry both. -->
              <span class="action-spacer"></span>
              {@render eyeToggle(panelKey, row.categoryId, row.name, true)}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/snippet}

{#if loading}
  <p>Loading…</p>
{:else}
  <div class="layout">
    {@render donutPanel({
      label: "Expenses",
      panelKey: "expense",
      total: expenseData.total,
      totalB: expenseData.totalB,
      slices: animatedExpenseSlices,
      txns: visibleExpenseTransactions,
      txnsB: visibleExpenseTransactionsB,
      allTxns: expenseTransactions,
      allTxnsB: reportableTransactionsB.filter((t) => t.amount_minor_units < 0),
      hidden: expenseHiddenRows,
    })}

    <div class="net-summary">
      <span class="net-summary-label">Left this period</span>
      <strong>{formatCurrency(netLeftMinorUnits, currency)}</strong>
      {#if compareActive}
        {@const delta = netLeftMinorUnits - netLeftMinorUnitsB}
        <!-- Money left over is the one figure on this page where more is
             simply better, whichever panel it came from — so unlike a
             category row it takes its verdict straight from the sign. -->
        <span class="net-delta {delta > 0 ? 'good' : delta < 0 ? 'bad' : 'flat'}">
          {formatDelta(delta, currency)}
        </span>
        <span class="net-was">was {formatCurrency(netLeftMinorUnitsB, currency)}</span>
      {/if}
    </div>

    {@render donutPanel({
      label: "Income",
      panelKey: "income",
      total: incomeData.total,
      totalB: incomeData.totalB,
      slices: animatedIncomeSlices,
      txns: visibleIncomeTransactions,
      txnsB: visibleIncomeTransactionsB,
      allTxns: incomeTransactions,
      allTxnsB: reportableTransactionsB.filter((t) => t.amount_minor_units > 0),
      hidden: incomeHiddenRows,
    })}
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
    flex-wrap: wrap;
    gap: 0.75rem;
    row-gap: 0.6rem;
    margin-bottom: 1.5rem;
  }

  .range-buttons {
    display: flex;
    gap: 0.4rem;
  }

  .range-buttons button {
    background-color: var(--color-shade-3);
    color: inherit;
    border: none;
    border-radius: 6px;
    padding: 0.45rem 0.9rem;
    font-size: 0.9rem;
    cursor: pointer;
  }

  .range-buttons button.active {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
  }

  .period-nav {
    display: flex;
    align-items: center;
    gap: 0.15rem;
  }

  .period-nav .nav-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    color: inherit;
    padding: 0.3rem;
    border-radius: 4px;
    cursor: pointer;
    opacity: 0.7;
    transition:
      opacity 0.15s ease,
      background-color 0.15s ease;
  }

  .period-nav .nav-button:hover:not(:disabled),
  .period-nav .nav-button:focus-visible:not(:disabled) {
    opacity: 1;
    background-color: var(--color-shade-3);
  }

  .period-nav .nav-button:disabled {
    opacity: 0.22;
    cursor: default;
  }

  .period-label {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.05rem;
    /* Wide enough that stepping between months of different name lengths
       doesn't shuffle the two arrows left and right under the pointer. */
    min-width: 11rem;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    padding: 0.15rem 0.4rem;
    border-radius: 6px;
    line-height: 1.2;
    cursor: pointer;
    transition: background-color 0.15s ease;
  }

  .period-label:disabled {
    cursor: default;
  }

  .period-label:not(:disabled):hover {
    background-color: var(--color-shade-3);
  }

  .period-name {
    font-size: 0.9rem;
    font-weight: 600;
    white-space: nowrap;
  }

  .period-label.stepped .period-name {
    color: var(--color-accent);
  }

  .period-span {
    font-size: 0.7rem;
    opacity: 0.55;
    white-space: nowrap;
  }

  .period-progress {
    font-size: 0.75rem;
    font-style: italic;
    opacity: 0.55;
    white-space: nowrap;
  }

  /* The comparison period's stepper is deliberately the same control as the
     primary one, just without the reset (there is no "now" for it to return
     to) — the two periods are peers, and giving the second one a different
     shape would suggest it is a setting rather than a period. */
  .period-label.static {
    cursor: default;
  }

  .vs {
    font-size: 0.8rem;
    opacity: 0.5;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .compare-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    margin-left: auto;
    background-color: var(--color-shade-3);
    color: inherit;
    border: none;
    border-radius: 6px;
    padding: 0.45rem 0.9rem;
    font-size: 0.9rem;
    font-family: inherit;
    cursor: pointer;
  }

  .compare-btn.active {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
  }

  .compare-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .mismatch {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin: -0.75rem 0 1.25rem;
    font-size: 0.82rem;
    color: #e0a33c;
  }

  /* Delta type is sized down from the amount beside it and coloured by whether
     the change is *good*, not by its sign — see `favourability`. `.flat` gets
     no colour at all: an unchanged figure has no verdict to give. */
  .delta {
    display: inline-flex;
    align-items: baseline;
    gap: 0.3rem;
    font-size: 0.8rem;
    font-weight: 600;
    white-space: nowrap;
  }

  /* Not `--color-success` / `--color-danger`: this app's danger colour is a
     light teal on purpose (see app.css — it has to sit beside the teal
     palette), and success *is* the accent, so the two resolve to nearly the
     same colour. A good/bad pair has to be opposites to be worth printing, so
     these take the accent's teal against the chart palette's coral — a hue
     already validated against this dark surface for CVD and contrast. The
     arrow in the text carries the same meaning independently, so nothing here
     rests on colour alone. */
  .delta.good,
  .net-delta.good,
  .center-delta.good {
    color: #00d6be;
  }

  .delta.bad,
  .net-delta.bad,
  .center-delta.bad {
    color: #ed613f;
  }

  .delta.flat,
  .net-delta.flat,
  .center-delta.flat {
    opacity: 0.5;
  }

  .delta-arrow {
    font-size: 0.62rem;
  }

  .delta-flat-text {
    font-size: 0.72rem;
    font-weight: 400;
    font-style: italic;
  }

  .delta-ratio {
    font-size: 0.72rem;
    font-weight: 400;
    opacity: 0.75;
  }

  /* Indented to clear the chevron and dot columns above it, so it hangs under
     the name it belongs to rather than starting a column of its own. */
  .prior-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.2rem;
    padding-left: 1.2rem;
    font-size: 0.78rem;
    opacity: 0.6;
  }

  /* Takes the place the category name holds on the line above, so this line's
     amount and percentage land in the same columns as the ones they are being
     compared with. */
  .prior-spacer {
    flex: 1;
    min-width: 0;
  }

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

  .prior-row .percent {
    font-weight: 400;
  }

  /* The row around it is dimmed to 0.6; the delta undoes that so it keeps its
     full colour and stays the first thing read on this line. */
  .prior-row .delta {
    opacity: 1;
    margin-left: 0.15rem;
  }

  .center-delta {
    margin-top: 0.15rem;
    font-size: 0.85rem;
    font-weight: 700;
  }

  .net-was {
    font-size: 0.65rem;
    opacity: 0.55;
    white-space: nowrap;
  }

  .panel-subtitle {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    margin-top: 0.2rem;
    color: var(--color-text);
    font-size: 0.72rem;
    font-weight: 400;
    opacity: 0.6;
    white-space: nowrap;
  }

  .stripe-swatch {
    width: 0.85rem;
    height: 0.5rem;
    border-radius: 2px;
    flex-shrink: 0;
    background-color: currentColor;
    background-image: repeating-linear-gradient(
      45deg,
      rgba(0, 0, 0, 0) 0 2px,
      rgba(0, 0, 0, 0.6) 2px 4px
    );
  }

  /* The comparison ring is the past: present but not competing with the ring
     in focus. Lower than the main ring's 0.82 and flat rather than glassed,
     so the eye lands on the outer ring first and finds this one second. */
  .compare-slice {
    opacity: 0.66;
  }

  .net-delta {
    display: block;
    margin-top: 0.2rem;
    font-size: 0.95rem;
    font-weight: 700;
  }

  .net-was {
    display: block;
    margin-top: 0.1rem;
  }

  .net-summary {
    /* Top-aligned with a fixed offset rather than centered on the grid row —
       the row's height includes the breakdown lists below, so centering on
       the row would push this down past the graphs. This offset lines it up
       with the vertical center of the donuts instead. */
    align-self: start;
    justify-self: center;
    margin-top: 7.5rem;
    text-align: center;
    padding: 1rem 1.25rem;
    border-radius: 10px;
    background-color: var(--color-box);
  }

  .net-summary-label {
    font-size: 0.85rem;
    opacity: 0.85;
  }

  .net-summary strong {
    display: block;
    margin-top: 0.3rem;
    font-size: 1.3rem;
    font-weight: 700;
    white-space: nowrap;
  }

  .layout {
    display: grid;
    /* `minmax(0, 1fr)`, not `1fr`: a bare `1fr` track floors at its content's
       min-width, and the comparison delta made a breakdown row wide enough to
       push the two columns past the page and out under the window edge. With
       the floor removed the track takes its half and the row absorbs it the
       way it was already built to — the legend gives width back, the category
       name ellipsises. */
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    gap: 2rem;
    align-items: start;
  }

  .panel-title {
    /* Sits in the donut's own grid track (row 1, column 1) rather than above
       the whole column: centered over the column as a whole, its middle landed
       in the gap between the donut and the legend, which read as a heading for
       neither. */
    grid-column: 1;
    grid-row: 1;
    text-align: center;
    font-size: 1rem;
    margin: 0 0 0.83rem;
  }

  .graph-graphics {
    /* The expense and income donuts must sit at the same spot in both columns
       whatever each one's category count is. As a plain flex row this was sized
       by its tallest/widest child — the legend — so the panel with more
       categories pushed its own donut down and sideways relative to the other.
       A fixed grid instead makes the legend a passenger that can never move the
       graph: the donut track is the donut's own size, and the row's height is
       that same size, so a long legend scrolls rather than growing the row.

       The legend track is minmax(0, 11rem) rather than a flat 11rem so it can
       give width back when the column is tight (the app's own default 1100px
       window is tight) instead of forcing the column wider and overflowing.
       Both columns are equal `1fr`, so the track resolves to the same width in
       each — the donuts stay aligned at every window size, just closer in. */
    --donut-size: min(260px, 60vw);
    display: grid;
    /* fallthrough — see `.graph-graphics.comparing` below */
    grid-template-columns: var(--donut-size) minmax(0, 11rem);
    /* The panel title takes row 1 of the donut's column so it is centered on
       the donut, not on donut+legend. Row 2 stays exactly the donut's size —
       what used to be this grid's fixed `height` — so the legend still has a
       definite height to scroll against instead of growing the row. */
    grid-template-rows: auto var(--donut-size);
    align-items: center;
    justify-content: center;
    column-gap: 1.5rem;
    row-gap: 0;
    margin-bottom: 1.5rem;
  }

  /* The donut deliberately does *not* grow when comparing. Growing it to buy
     back the room the inner ring takes was tried, and at the app's own default
     1100px window it ate the legend's track instead — "Dining out" became
     "Dining …". The middle got its room from losing a line rather than from
     taking one off the legend: the comparison total moved up into the panel
     header (see `.panel-subtitle`), where it does a second job the centre
     never could — naming the striped ring. */

  .donut-wrap {
    /* scoped to the component, not :root — a page-level :root block is
       unmounted on navigation and takes its variables with it */
    --donut-track: rgba(255, 255, 255, 0.07);
    grid-column: 1;
    grid-row: 2;
    position: relative;
    width: var(--donut-size);
  }

  .donut {
    width: 100%;
    height: auto;
    display: block;
    /* above the glass disc painted by .donut-wrap::before */
    position: relative;
    z-index: 1;
  }

  /* The disc the ring sits on. On a pure-black page there is nothing behind it
     to refract, so the glass reads through the tinted fill, the hairline edge
     and the sheen rather than through the blur — the blur only matters where
     the panel overlaps content (hover states, the range bar on short viewports). */
  .donut-wrap::before {
    content: "";
    position: absolute;
    inset: 4%;
    border-radius: 50%;
    background: radial-gradient(
      circle at 32% 25%,
      rgba(255, 255, 255, 0.09),
      rgba(255, 255, 255, 0.025) 45%,
      rgba(255, 255, 255, 0.012) 100%
    );
    border: 1px solid rgba(255, 255, 255, 0.08);
    backdrop-filter: blur(14px);
  }

  .slice {
    /* 0.82 keeps the least luminous slot (#007fa9) at ~3.2:1 against the
       disc — translucent enough to read as glass, opaque enough to stay
       above the 3:1 floor for chart marks. The bloom filter puts back the
       light the transparency takes away. */
    opacity: 0.82;
    transition:
      opacity 0.15s ease,
      stroke-width 0.15s ease;
    cursor: pointer;
  }

  .slice.dimmed {
    opacity: 0.28;
  }

  .legend {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    /* Fills its grid track rather than sizing to its own content: a legend that
       shrank to fit two short category names would slide its donut sideways
       while the other panel's stayed put. It also stays rendered (empty) when a
       panel has no slices, so that track keeps its width. The row gives the
       height a definite 100% to resolve against, so a long list scrolls here
       instead of growing the row and dragging the donut down with it. */
    grid-column: 2;
    grid-row: 2;
    min-width: 0;
    max-height: 100%;
    overflow-y: auto;
    font-size: 0.8rem;
  }

  .legend li {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.05rem 0.15rem;
    border-radius: 4px;
    cursor: pointer;
    transition:
      opacity 0.15s ease,
      background-color 0.15s ease;
  }

  .legend li .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .legend li.dimmed {
    opacity: 0.4;
  }

  .legend li:hover {
    background-color: var(--color-shade-3);
  }

  .donut-center {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 0.15rem;
    /* This sits on top of the donut SVG (inset:0 over the whole wrap) — without
       this it would swallow every pointer event before the slices underneath
       ever see a hover, even though only the center text is visually here. */
    pointer-events: none;
  }

  .donut-center .total {
    font-size: 1.4rem;
    font-weight: 700;
  }

  .donut-center .label {
    font-size: 0.8rem;
    opacity: 0.7;
    text-transform: uppercase;
  }

  /* The hole is a circle, so the usable width narrows the further a line sits
     from the middle — which is why the outer two lines step down hardest. */
  .donut-center.compact {
    gap: 0.05rem;
    padding: 0 12%;
  }

  .donut-center.compact .total {
    font-size: 1.1rem;
  }

  .donut-center.compact .label {
    font-size: 0.62rem;
  }

  .breakdown {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .breakdown li {
    transition: opacity 0.15s ease;
  }

  .breakdown li.dimmed {
    opacity: 0.4;
  }

  .breakdown-row {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .row-main {
    display: block;
    /* min-width:0 so a long category name ellipsises inside the flex row
       instead of pushing the eye button off the edge of the column */
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    border-radius: 4px;
    padding: 0.2rem 0.3rem;
    margin: -0.2rem 0 -0.2rem -0.3rem;
    cursor: default;
    transition: background-color 0.15s ease;
  }

  .row-main.clickable {
    cursor: pointer;
  }

  .row-main.clickable:hover {
    background-color: var(--color-shade-3);
  }

  /* The eye is an always-visible affordance rather than hover-only: it's the
     page's only remaining way to filter a category, so it can't be
     undiscoverable — but it sits back at low opacity until pointed at.
     `.goto-btn` shares the treatment so the row's two actions read as a pair
     rather than as one control plus something else. */
  .eye-btn,
  .goto-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    background: none;
    border: none;
    color: inherit;
    padding: 0.25rem;
    border-radius: 4px;
    cursor: pointer;
    opacity: 0.35;
    transition:
      opacity 0.15s ease,
      background-color 0.15s ease;
  }

  .eye-btn:hover,
  .eye-btn:focus-visible,
  .goto-btn:hover,
  .goto-btn:focus-visible {
    opacity: 1;
    background-color: var(--color-shade-3);
  }

  .eye-btn.is-hidden {
    opacity: 0.8;
  }

  /* It's an <a> for real navigation semantics, so the anchor defaults
     (underline, link colour) have to be undone to match the eye beside it. */
  .goto-btn {
    text-decoration: none;
  }

  /* One row action's worth of width, for rows that deliberately don't offer
     that action — keeps every row's action column landing in the same place
     regardless of which of them a given row actually has. */
  .action-spacer {
    flex-shrink: 0;
    width: 1.5rem;
  }

  .eye {
    width: 1rem;
    height: 1rem;
    display: block;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .hidden-cat .row-main {
    opacity: 0.45;
  }

  .hidden-cat .name {
    text-decoration: line-through;
  }

  .hidden-cat .row {
    margin-bottom: 0;
  }

  .dot.muted {
    background-color: var(--color-shade-3);
  }

  .chevron {
    display: inline-block;
    width: 0.7rem;
    flex-shrink: 0;
    text-align: center;
    opacity: 0.6;
    transition: transform 0.15s ease;
  }

  .chevron.expanded {
    transform: rotate(90deg);
  }

  .chevron.invisible {
    visibility: hidden;
  }

  .sub-breakdown {
    list-style: none;
    margin: 0.5rem 0 0;
    padding-left: 1.4rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    border-left: 1px solid var(--color-shade-3);
  }

  .sub-row {
    padding-left: 0.75rem;
  }

  .sub-row .name {
    font-size: 0.9rem;
    opacity: 0.9;
  }

  .sub-row .percent {
    font-size: 0.85rem;
  }

  /* Sits inline right after `.percent`, deliberately lighter and unbolded:
     the two numbers share a row, so weight and opacity are what tell them
     apart at a glance rather than a sentence spelling out each denominator. */
  .percent-of-total {
    /* pulls back the .row flex gap so the "·" reads as joining the two
       percentages rather than separating three equal-weight columns */
    margin-left: -0.25rem;
    font-size: 0.75rem;
    font-weight: 400;
    opacity: 0.6;
    white-space: nowrap;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.25rem;
  }

  .dot {
    width: 0.6rem;
    height: 0.6rem;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .name {
    flex: 1;
  }

  /* Both nowrap: with a delta chip competing for the row's width, "€2 500,00"
     was breaking after the "€2" and reading as two numbers. `.name` is the
     only thing in the row that may give ground, and it ellipsises. */
  .amount {
    opacity: 0.75;
    white-space: nowrap;
  }

  .percent {
    font-weight: 600;
    white-space: nowrap;
  }

  /* `min-width: 0` is what actually lets it ellipsise: a flex item defaults to
     `min-width: auto`, which refuses to shrink below its own text and pushes
     the row's trailing actions off the edge of the column instead. */
  .row .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* A holder for one or two lanes rather than a bar itself. Not comparing,
     it is a single lane and looks exactly as it always did. */
  .bar-track {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .bar-lane {
    height: 0.4rem;
    border-radius: 999px;
    background-color: var(--color-shade-3);
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    border-radius: 999px;
  }

  /* The comparison period, in the category's colour with the colour cut away
     on the diagonal. Both lanes share a left edge and a scale, so which is
     longer is the whole reading — the stripes only have to say which one is
     the past, not carry a value of their own.

     45deg because a vertical hatch on a bar reads as tick marks (i.e. as a
     scale) and a horizontal one disappears at this height. The opacity keeps
     it under the solid lane in the visual hierarchy without weakening its
     edge, which is the part being compared. */
  .bar-fill.prior {
    background-image: repeating-linear-gradient(
      45deg,
      rgba(0, 0, 0, 0) 0 3px,
      rgba(0, 0, 0, 0.55) 3px 6px
    );
    opacity: 0.8;
  }

  .bar-fill {
    height: 100%;
  }

  @media (max-width: 640px) {
    .layout {
      grid-template-columns: 1fr;
    }

    .net-summary {
      margin-top: 0;
    }
  }
</style>
