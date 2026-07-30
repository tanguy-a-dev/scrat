<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    formatCurrency,
    computeRange,
    todayIsoDate,
    type CategoryDto,
    type TransactionDto,
    type RangeMode,
  } from "$lib/api";

  // Validated categorical palette (dark-mode steps) — passes CVD/contrast
  // checks against this app's dark surface. See dataviz skill's palette.md.
  const PALETTE = [
    "#3987e5",
    "#d95926",
    "#199e70",
    "#c98500",
    "#d55181",
    "#008300",
    "#9085e9",
    "#e66767",
  ];
  const RADIUS = 80;
  const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

  let categories = $state<CategoryDto[]>([]);
  let transactions = $state<TransactionDto[]>([]);
  let loading = $state(true);
  let error = $state("");

  let rangeMode = $state<RangeMode>("month");
  let customStart = $state(todayIsoDate());
  let customEnd = $state(todayIsoDate());

  let excludedRootIds = $state<Set<string>>(new Set());

  // Hovering the donut slice or the legend entry for a category highlights
  // both plus the matching breakdown row — but not the reverse: hovering the
  // breakdown row only highlights itself, it never drives the graph/legend.
  let hoveredCategoryId = $state<string | null>(null);

  type PanelKey = "expense" | "income";

  // Which root category (if any) each panel is drilled into, showing that
  // category's subcategories instead of the root-level breakdown.
  let drilldown = $state<Record<PanelKey, string | null>>({
    expense: null,
    income: null,
  });

  function categoryHasChildren(id: string): boolean {
    return categories.some((c) => c.parent_id === id);
  }

  function drillInto(panel: PanelKey, categoryId: string) {
    if (!categoryHasChildren(categoryId)) return;
    drilldown = { ...drilldown, [panel]: categoryId };
  }

  function drillBack(panel: PanelKey) {
    drilldown = { ...drilldown, [panel]: null };
  }

  // If the category a panel is drilled into gets excluded via the filters
  // list, drop back to the root view rather than showing a dead-end empty
  // drilldown.
  $effect(() => {
    if (drilldown.expense && excludedRootIds.has(drilldown.expense)) {
      drilldown = { ...drilldown, expense: null };
    }
    if (drilldown.income && excludedRootIds.has(drilldown.income)) {
      drilldown = { ...drilldown, income: null };
    }
  });

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
    drilldown = { expense: null, income: null };
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

  function toggleRootCategory(id: string) {
    const next = new Set(excludedRootIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    excludedRootIds = next;
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

  // Only categories with at least one transaction in the selected date
  // range are worth showing as a filter — an empty one is just noise.
  let presentRootIds = $derived.by(() => {
    const ids = new Set<string>();
    for (const t of transactions) ids.add(rootCategoryId(t.category_id));
    return ids;
  });

  let rootCategories = $derived(
    categories.filter((c) => c.parent_id === null && presentRootIds.has(c.id)),
  );

  let filteredTransactions = $derived(
    transactions.filter((t) => !excludedRootIds.has(rootCategoryId(t.category_id))),
  );

  let currency = $derived(transactions[0]?.currency ?? "EUR");

  let netLeftMinorUnits = $derived(
    filteredTransactions.reduce((sum, t) => sum + t.amount_minor_units, 0),
  );

  let expenseTransactions = $derived(
    filteredTransactions.filter((t) => t.amount_minor_units < 0),
  );
  let incomeTransactions = $derived(
    filteredTransactions.filter((t) => t.amount_minor_units > 0),
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

  function withDonutSlices(breakdown: ReturnType<typeof buildBreakdown>["breakdown"]) {
    let cumulative = 0;
    return breakdown.map((slice, i) => {
      const length = (slice.percent / 100) * CIRCUMFERENCE;
      const dashoffset = -cumulative;
      cumulative += length;
      return {
        ...slice,
        color: PALETTE[i % PALETTE.length],
        dasharray: `${length} ${CIRCUMFERENCE - length}`,
        dashoffset,
      };
    });
  }

  // Scales each slice's arc length/offset and each bar's width by
  // `fillProgress`, so the whole donut sweeps in from empty together rather
  // than each slice animating independently out of sync with the others.
  function withAnimatedSlices(slices: ReturnType<typeof withDonutSlices>) {
    return slices.map((slice) => {
      const animatedLength = (slice.percent / 100) * CIRCUMFERENCE * fillProgress;
      return {
        ...slice,
        animatedDasharray: `${animatedLength} ${CIRCUMFERENCE - animatedLength}`,
        animatedDashoffset: slice.dashoffset * fillProgress,
        animatedPercent: slice.percent * fillProgress,
      };
    });
  }

  let expenseData = $derived.by(() => buildBreakdown(expenseTransactions, drilldown.expense));
  let incomeData = $derived.by(() => buildBreakdown(incomeTransactions, drilldown.income));

  let animatedExpenseSlices = $derived(
    withAnimatedSlices(withDonutSlices(expenseData.breakdown)),
  );
  let animatedIncomeSlices = $derived(
    withAnimatedSlices(withDonutSlices(incomeData.breakdown)),
  );

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
    <input type="date" bind:value={customStart} onchange={load} />
    <span>to</span>
    <input type="date" bind:value={customEnd} onchange={load} />
  {/if}
</div>

{#snippet donutPanel(
  label: string,
  panelKey: PanelKey,
  total: number,
  slices: typeof animatedExpenseSlices,
)}
  <div class="graph-column">
    <h2 class="panel-title">
      {#if drilldown[panelKey]}
        <span class="breadcrumb">
          <button type="button" class="back-link" onclick={() => drillBack(panelKey)}
            >{label}</button
          >
          <span class="crumb-sep">›</span>
          {categoryName(drilldown[panelKey]!)}
        </span>
      {:else}
        {label}
      {/if}
    </h2>
    <div class="graph-graphics">
      <div class="donut-wrap">
        <svg viewBox="0 0 200 200" class="donut">
          <g transform="rotate(-90 100 100)">
            <circle
              cx="100"
              cy="100"
              r={RADIUS}
              fill="none"
              stroke="var(--donut-track)"
              stroke-width="24"
            />
            {#each slices as slice (slice.categoryId)}
              <circle
                cx="100"
                cy="100"
                r={RADIUS}
                fill="none"
                stroke={slice.color}
                stroke-width="24"
                stroke-dasharray={slice.animatedDasharray}
                stroke-dashoffset={slice.animatedDashoffset}
                class="slice"
                class:dimmed={hoveredCategoryId !== null && hoveredCategoryId !== slice.categoryId}
                role="presentation"
                onmouseenter={() => (hoveredCategoryId = slice.categoryId)}
                onmouseleave={() => (hoveredCategoryId = null)}
              />
            {/each}
          </g>
        </svg>
        <div class="donut-center">
          <span class="total">{formatCurrency(total, currency)}</span>
          <span class="label">{drilldown[panelKey] ? categoryName(drilldown[panelKey]!) : label}</span>
        </div>
      </div>

      {#if slices.length > 0}
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
      {/if}
    </div>

    {#if slices.length === 0}
      <p class="empty">No {label.toLowerCase()} in this range.</p>
    {:else}
      <ul class="breakdown">
        {#each slices as slice (slice.categoryId)}
          <li
            class:dimmed={hoveredCategoryId !== null && hoveredCategoryId !== slice.categoryId}
          >
            <button
              type="button"
              class="breakdown-row"
              class:clickable={categoryHasChildren(slice.categoryId)}
              disabled={!categoryHasChildren(slice.categoryId)}
              onclick={() => drillInto(panelKey, slice.categoryId)}
            >
              <div class="row">
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
    {@render donutPanel("Expenses", "expense", expenseData.total, animatedExpenseSlices)}

    <div class="net-summary">
      Left this period
      <strong>{formatCurrency(netLeftMinorUnits, currency)}</strong>
    </div>

    {@render donutPanel("Income", "income", incomeData.total, animatedIncomeSlices)}

    <aside class="filters">
      <h2>Categories</h2>
      {#if rootCategories.length === 0}
        <p class="empty">No categories yet.</p>
      {:else}
        <ul>
          {#each rootCategories as cat (cat.id)}
            <li>
              <label>
                <input
                  type="checkbox"
                  checked={!excludedRootIds.has(cat.id)}
                  onchange={() => toggleRootCategory(cat.id)}
                />
                {cat.name}
              </label>
            </li>
          {/each}
        </ul>
      {/if}
    </aside>
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
    align-self: center;
    justify-self: center;
    text-align: center;
    font-size: 0.85rem;
    opacity: 0.85;
  }

  .net-summary strong {
    display: block;
    margin-top: 0.3rem;
    font-size: 1.3rem;
    font-weight: 700;
    white-space: nowrap;
    opacity: 1;
  }

  input[type="date"] {
    border-radius: 6px;
    border: 1px solid var(--color-shade-3);
    background-color: var(--color-shade-2);
    color: inherit;
    padding: 0.4rem 0.6rem;
    font-family: inherit;
  }

  .layout {
    display: grid;
    grid-template-columns: 1fr auto 1fr 16rem;
    gap: 2rem;
    align-items: start;
  }

  .panel-title {
    text-align: center;
    font-size: 1rem;
    margin-top: 0;
  }

  .breadcrumb {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }

  .back-link {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--color-accent);
    cursor: pointer;
  }

  .back-link:hover {
    text-decoration: underline;
  }

  .crumb-sep {
    opacity: 0.6;
  }

  .graph-graphics {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 1.5rem;
    margin-bottom: 1.5rem;
  }

  .donut-wrap {
    position: relative;
    width: min(260px, 60vw);
    flex-shrink: 0;
  }

  .donut {
    width: 100%;
    height: auto;
    display: block;
  }

  .slice {
    transition:
      opacity 0.15s ease,
      stroke-width 0.15s ease;
    cursor: pointer;
  }

  .slice.dimmed {
    opacity: 0.35;
  }

  :root {
    --donut-track: var(--color-shade-3);
  }

  .legend {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    max-width: 8.5rem;
    font-size: 0.8rem;
  }

  .legend li {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.1rem 0.2rem;
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
    display: block;
    width: 100%;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    border-radius: 4px;
    padding: 0.2rem 0.3rem;
    margin: -0.2rem -0.3rem;
    cursor: default;
    transition: background-color 0.15s ease;
  }

  .breakdown-row.clickable {
    cursor: pointer;
  }

  .breakdown-row.clickable:hover {
    background-color: var(--color-shade-3);
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

  .filters {
    border-left: 1px solid var(--color-shade-3);
    padding-left: 1.5rem;
  }

  .filters h2 {
    font-size: 1rem;
    margin-top: 0;
  }

  .filters ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .filters label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  .filters input[type="checkbox"] {
    accent-color: var(--color-accent);
  }

  @media (max-width: 1100px) {
    .layout {
      grid-template-columns: 1fr auto 1fr;
    }

    .filters {
      grid-column: 1 / -1;
      border-left: none;
      padding-left: 0;
      border-top: 1px solid var(--color-shade-3);
      padding-top: 1rem;
    }
  }

  @media (max-width: 640px) {
    .layout {
      grid-template-columns: 1fr;
    }
  }
</style>
