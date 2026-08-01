<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    countsTowardTotals,
    formatCurrency,
    computeRange,
    todayIsoDate,
    oneMonthAgoIsoDate,
    type CategoryDto,
    type TransactionDto,
    type RangeMode,
  } from "$lib/api";
  import DateRangePicker from "$lib/DateRangePicker.svelte";

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

  // Shrink an arc by the gap, but never past half its own length — a very
  // small slice should stay visible rather than be eaten by the spacer.
  // A lone slice has nothing to be separated from, so it keeps the full ring.
  function gapped(length: number, sliceCount: number): number {
    if (sliceCount < 2) return length;
    return Math.max(length - SLICE_GAP, length / 2);
  }

  let categories = $state<CategoryDto[]>([]);
  let transactions = $state<TransactionDto[]>([]);
  let loading = $state(true);
  let error = $state("");

  let rangeMode = $state<RangeMode>("month");
  let customStart = $state(oneMonthAgoIsoDate());
  let customEnd = $state(todayIsoDate());

  // Hovering the donut slice or the legend entry for a category highlights
  // both plus the matching breakdown row — but not the reverse: hovering the
  // breakdown row only highlights itself, it never drives the graph/legend.
  let hoveredCategoryId = $state<string | null>(null);

  type PanelKey = "expense" | "income";

  // Which root categories are expanded, per panel — an expanded row shows
  // its subcategories as extra rows underneath it, inline in the same list,
  // rather than replacing the panel's own root-level breakdown.
  let expandedCategoryIds = $state<Record<PanelKey, Set<string>>>({
    expense: new Set(),
    income: new Set(),
  });

  // Categories the user has hidden via the eye toggle, per panel. Hiding is
  // per-panel because the toggle lives on a panel's own breakdown row: hiding
  // a category from Expenses shouldn't silently reshape the Income donut.
  // A hidden category's transactions drop out of that panel's total, so every
  // other category's percentage recomputes over what's left.
  let hiddenCategoryIds = $state<Record<PanelKey, Set<string>>>({
    expense: new Set(),
    income: new Set(),
  });

  function categoryHasChildren(id: string): boolean {
    return categories.some((c) => c.parent_id === id);
  }

  // A category is only worth expanding if doing so would reveal a genuinely
  // different breakdown — i.e. at least one of its transactions is actually
  // assigned to a child category. A category with subcategories defined but
  // whose transactions are all logged directly against it (e.g. Transportation
  // with no transaction ever assigned to a specific subcategory) would just
  // show itself again under itself, which isn't useful.
  function hasVisibleSubcategories(txns: TransactionDto[], rootId: string): boolean {
    if (!categoryHasChildren(rootId)) return false;
    return txns.some(
      (t) => t.category_id !== rootId && rootCategoryId(t.category_id) === rootId,
    );
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

  async function load() {
    loading = true;
    error = "";
    expandedCategoryIds = { expense: new Set(), income: new Set() };
    try {
      const range = computeRange(rangeMode, {
        start: customStart,
        end: customEnd,
      });
      const [c, t] = await Promise.all([
        api.listCategories(),
        api.listTransactions(range.start, range.end),
      ]);
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

  let rootMap = $derived.by(() => {
    const byId = new Map(categories.map((c) => [c.id, c]));
    const cache = new Map<string, string>();
    function findRoot(id: string): string {
      if (cache.has(id)) return cache.get(id)!;
      const cat = byId.get(id);
      const root = cat?.parent_id ? findRoot(cat.parent_id) : id;
      cache.set(id, root);
      return root;
    }
    for (const c of categories) findRoot(c.id);
    return cache;
  });

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

  let netLeftMinorUnits = $derived(
    [...visibleExpenseTransactions, ...visibleIncomeTransactions].reduce(
      (sum, t) => sum + t.amount_minor_units,
      0,
    ),
  );

  // scopeRootId narrows to one root category's transactions and groups by
  // its subcategories instead of by root — this is what powers drilldown.
  function buildBreakdown(txns: TransactionDto[], scopeRootId: string | null) {
    const scoped = scopeRootId
      ? txns.filter((t) => rootCategoryId(t.category_id) === scopeRootId)
      : txns;
    const total = scoped.reduce((sum, t) => sum + Math.abs(t.amount_minor_units), 0);
    const sums = new Map<string, number>();
    for (const t of scoped) {
      const key = scopeRootId ? t.category_id : rootCategoryId(t.category_id);
      sums.set(key, (sums.get(key) ?? 0) + Math.abs(t.amount_minor_units));
    }
    const totalOrOne = total || 1;
    const breakdown = [...sums.entries()]
      .map(([categoryId, amountMinorUnits]) => ({
        categoryId,
        name: categoryName(categoryId),
        amountMinorUnits,
        percent: (amountMinorUnits / totalOrOne) * 100,
      }))
      .sort((a, b) => b.amountMinorUnits - a.amountMinorUnits);
    return { total, breakdown };
  }

  // Rows for the categories this panel is currently hiding: they carry their
  // real amount (so the user can see what they're leaving out) but no share,
  // since by definition they're no longer part of the total the shares are of.
  function hiddenRows(
    panel: PanelKey,
    txns: TransactionDto[],
    scopeRootId: string | null,
  ) {
    const hidden = hiddenCategoryIds[panel];
    const sums = new Map<string, number>();
    for (const t of txns) {
      const root = rootCategoryId(t.category_id);
      if (scopeRootId !== null && root !== scopeRootId) continue;
      const key = scopeRootId ? t.category_id : root;
      if (!hidden.has(key)) continue;
      sums.set(key, (sums.get(key) ?? 0) + Math.abs(t.amount_minor_units));
    }
    return [...sums.entries()]
      .map(([categoryId, amountMinorUnits]) => ({
        categoryId,
        name: categoryName(categoryId),
        amountMinorUnits,
      }))
      .sort((a, b) => b.amountMinorUnits - a.amountMinorUnits);
  }

  // Colored (but non-animated) breakdown of one root category's subcategories,
  // for rendering the expanded rows nested under it in the breakdown list.
  // Each row gets two percentages: `percent` is share of the parent category
  // (e.g. Rent's share of Housing), `percentOfTotal` is share of the whole
  // panel (e.g. Rent's share of all Expenses) — the two answer different
  // questions and both are useful side by side.
  function subCategoryBreakdown(txns: TransactionDto[], rootId: string, panelTotal: number) {
    const panelTotalOrOne = panelTotal || 1;
    const withTotalShare = buildBreakdown(txns, rootId).breakdown.map((slice) => ({
      ...slice,
      percentOfTotal: (slice.amountMinorUnits / panelTotalOrOne) * 100,
    }));
    return withDonutSlices(withTotalShare);
  }

  function withDonutSlices<T extends { percent: number }>(breakdown: T[]) {
    let cumulative = 0;
    return breakdown.map((slice, i) => {
      const length = (slice.percent / 100) * CIRCUMFERENCE;
      const dashoffset = -cumulative;
      cumulative += length;
      const drawn = gapped(length, breakdown.length);
      return {
        ...slice,
        color: PALETTE[i % PALETTE.length],
        dasharray: `${drawn} ${CIRCUMFERENCE - drawn}`,
        dashoffset,
      };
    });
  }

  // Scales each slice's arc length/offset and each bar's width by
  // `fillProgress`, so the whole donut sweeps in from empty together rather
  // than each slice animating independently out of sync with the others.
  function withAnimatedSlices<T extends { percent: number; dashoffset: number }>(slices: T[]) {
    return slices.map((slice) => {
      const animatedLength = gapped(
        (slice.percent / 100) * CIRCUMFERENCE * fillProgress,
        slices.length,
      );
      return {
        ...slice,
        animatedLength,
        animatedDashoffset: slice.dashoffset * fillProgress,
        animatedPercent: slice.percent * fillProgress,
      };
    });
  }

  let expenseData = $derived.by(() => buildBreakdown(visibleExpenseTransactions, null));
  let incomeData = $derived.by(() => buildBreakdown(visibleIncomeTransactions, null));

  let animatedExpenseSlices = $derived(
    withAnimatedSlices(withDonutSlices(expenseData.breakdown)),
  );
  let animatedIncomeSlices = $derived(
    withAnimatedSlices(withDonutSlices(incomeData.breakdown)),
  );

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
</div>

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

{#snippet donutPanel(
  label: string,
  panelKey: PanelKey,
  total: number,
  slices: typeof animatedExpenseSlices,
  txns: TransactionDto[],
  allTxns: TransactionDto[],
  hidden: { categoryId: string; name: string; amountMinorUnits: number }[],
)}
  <div class="graph-column">
    <h2 class="panel-title">{label}</h2>
    <div class="graph-graphics">
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
        <div class="donut-center">
          <span class="total">{formatCurrency(total, currency)}</span>
          <span class="label">{label}</span>
        </div>
      </div>

      <!-- Rendered even when empty: it holds the column that keeps this panel's
           donut horizontally aligned with the other panel's. -->
      <ul class="legend">
        {#each slices as slice (slice.categoryId)}
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
          {@const hasChildren = hasVisibleSubcategories(allTxns, slice.categoryId)}
          {@const expanded = expandedCategoryIds[panelKey].has(slice.categoryId)}
          {@const subHidden = hiddenRows(panelKey, allTxns, slice.categoryId)}
          <li
            class:dimmed={hoveredCategoryId !== null && hoveredCategoryId !== slice.categoryId}
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
                <div class="bar-track">
                  <div
                    class="bar-fill"
                    style={`width:${slice.animatedPercent}%;background-color:${slice.color}`}
                  ></div>
                </div>
              </button>
              {@render eyeToggle(panelKey, slice.categoryId, slice.name, false)}
            </div>

            {#if hasChildren && expanded}
              <ul class="sub-breakdown">
                {#each subCategoryBreakdown(txns, slice.categoryId, total) as sub (sub.categoryId)}
                  <li class="sub-row">
                    <div class="breakdown-row">
                      <div class="row-main">
                        <div class="row">
                          <span class="dot" style={`background-color:${sub.color}`}></span>
                          <span class="name">{sub.name}</span>
                          <span class="amount"
                            >{formatCurrency(sub.amountMinorUnits, currency)}</span
                          >
                          <span class="percent">{sub.percent.toFixed(1)}% of {slice.name}</span>
                        </div>
                        <div class="bar-track">
                          <div
                            class="bar-fill"
                            style={`width:${sub.percent}%;background-color:${sub.color}`}
                          ></div>
                        </div>
                        <div class="percent-of-total">
                          {sub.percentOfTotal.toFixed(1)}% of total {label.toLowerCase()}
                        </div>
                      </div>
                      <!-- A parent's transactions logged directly against it
                           (rather than a child) surface as a sub-row carrying
                           the parent's own id — so an eye here would be the
                           parent's eye, hiding the whole branch from inside
                           itself. The parent row above already owns that
                           toggle; this row just gets a spacer to stay aligned. -->
                      {#if sub.categoryId === slice.categoryId}
                        <span class="eye-spacer"></span>
                      {:else}
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
    {@render donutPanel(
      "Expenses",
      "expense",
      expenseData.total,
      animatedExpenseSlices,
      visibleExpenseTransactions,
      expenseTransactions,
      expenseHiddenRows,
    )}

    <div class="net-summary">
      <span class="net-summary-label">Left this period</span>
      <strong>{formatCurrency(netLeftMinorUnits, currency)}</strong>
    </div>

    {@render donutPanel(
      "Income",
      "income",
      incomeData.total,
      animatedIncomeSlices,
      visibleIncomeTransactions,
      incomeTransactions,
      incomeHiddenRows,
    )}
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
    grid-template-columns: 1fr auto 1fr;
    gap: 2rem;
    align-items: start;
  }

  .panel-title {
    text-align: center;
    font-size: 1rem;
    margin-top: 0;
  }

  .graph-graphics {
    /* The expense and income donuts must sit at the same spot in both columns
       whatever each one's category count is. As a plain flex row this was sized
       by its tallest/widest child — the legend — so the panel with more
       categories pushed its own donut down and sideways relative to the other.
       A fixed grid instead makes the legend a passenger that can never move the
       graph: the donut track is the donut's own size, and the row's height is
       that same size, so a long legend scrolls rather than growing the row.

       The legend track is minmax(0, 8.5rem) rather than a flat 8.5rem so it can
       give width back when the column is tight (the app's own default 1100px
       window is tight) instead of forcing the column wider and overflowing.
       Both columns are equal `1fr`, so the track resolves to the same width in
       each — the donuts stay aligned at every window size, just closer in. */
    --donut-size: min(260px, 60vw);
    display: grid;
    grid-template-columns: var(--donut-size) minmax(0, 8.5rem);
    align-items: center;
    justify-content: center;
    gap: 1.5rem;
    height: var(--donut-size);
    margin-bottom: 1.5rem;
  }

  .donut-wrap {
    /* scoped to the component, not :root — a page-level :root block is
       unmounted on navigation and takes its variables with it */
    --donut-track: rgba(255, 255, 255, 0.07);
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
     undiscoverable — but it sits back at low opacity until pointed at. */
  .eye-btn {
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
  .eye-btn:focus-visible {
    opacity: 1;
    background-color: var(--color-shade-3);
  }

  .eye-btn.is-hidden {
    opacity: 0.8;
  }

  .eye-spacer {
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

  .percent-of-total {
    margin-top: 0.25rem;
    font-size: 0.75rem;
    opacity: 0.65;
    text-align: right;
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

  .amount {
    opacity: 0.75;
  }

  .percent {
    font-weight: 600;
  }

  .bar-track {
    height: 0.4rem;
    border-radius: 999px;
    background-color: var(--color-shade-3);
    overflow: hidden;
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
