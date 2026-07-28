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

  const PALETTE = [
    "#228be6",
    "#97f2d7",
    "#f2cc8f",
    "#e07a5f",
    "#9b5de5",
    "#43aa8b",
    "#f15bb5",
    "#98abb5",
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

  let activeTab = $state<"expenses" | "income">("expenses");
  let excludedRootIds = $state<Set<string>>(new Set());

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

  let activeTransactions = $derived(
    filteredTransactions.filter((t) =>
      activeTab === "expenses" ? t.amount_minor_units < 0 : t.amount_minor_units > 0,
    ),
  );

  let totalForTabMinorUnits = $derived(
    activeTransactions.reduce((sum, t) => sum + Math.abs(t.amount_minor_units), 0),
  );

  let breakdown = $derived.by(() => {
    const sums = new Map<string, number>();
    for (const t of activeTransactions) {
      const root = rootCategoryId(t.category_id);
      sums.set(root, (sums.get(root) ?? 0) + Math.abs(t.amount_minor_units));
    }
    const total = totalForTabMinorUnits || 1;
    return [...sums.entries()]
      .map(([categoryId, amountMinorUnits]) => ({
        categoryId,
        name: categoryName(categoryId),
        amountMinorUnits,
        percent: (amountMinorUnits / total) * 100,
      }))
      .sort((a, b) => b.amountMinorUnits - a.amountMinorUnits);
  });

  let donutSlices = $derived.by(() => {
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
  });

  // Scales each slice's arc length/offset and each bar's width by
  // `fillProgress`, so the whole donut sweeps in from empty together rather
  // than each slice animating independently out of sync with the others.
  let animatedDonutSlices = $derived(
    donutSlices.map((slice) => {
      const animatedLength = (slice.percent / 100) * CIRCUMFERENCE * fillProgress;
      return {
        ...slice,
        animatedDasharray: `${animatedLength} ${CIRCUMFERENCE - animatedLength}`,
        animatedDashoffset: slice.dashoffset * fillProgress,
        animatedPercent: slice.percent * fillProgress,
      };
    }),
  );

  $effect(() => {
    breakdown;
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

{#if loading}
  <p>Loading…</p>
{:else}
  <div class="layout">
    <div class="graph-column">
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
            {#each animatedDonutSlices as slice (slice.categoryId)}
              <circle
                cx="100"
                cy="100"
                r={RADIUS}
                fill="none"
                stroke={slice.color}
                stroke-width="24"
                stroke-dasharray={slice.animatedDasharray}
                stroke-dashoffset={slice.animatedDashoffset}
              />
            {/each}
          </g>
        </svg>
        <div class="donut-center">
          <span class="total">{formatCurrency(totalForTabMinorUnits, currency)}</span>
          <span class="label">{activeTab === "expenses" ? "Expenses" : "Income"}</span>
          <span class="left">Left: {formatCurrency(netLeftMinorUnits, currency)}</span>
        </div>
      </div>

      <div class="tabs">
        <button
          type="button"
          class:active={activeTab === "expenses"}
          onclick={() => (activeTab = "expenses")}>Expenses</button
        >
        <button
          type="button"
          class:active={activeTab === "income"}
          onclick={() => (activeTab = "income")}>Income</button
        >
      </div>

      {#if breakdown.length === 0}
        <p class="empty">No {activeTab} in this range.</p>
      {:else}
        <ul class="breakdown">
          {#each animatedDonutSlices as slice (slice.categoryId)}
            <li>
              <div class="row">
                <span class="dot" style={`background-color:${slice.color}`}></span>
                <span class="name">{slice.name}</span>
                <span class="percent">{slice.percent.toFixed(1)}%</span>
              </div>
              <div class="bar-track">
                <div
                  class="bar-fill"
                  style={`width:${slice.animatedPercent}%;background-color:${slice.color}`}
                ></div>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

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

  .range-buttons,
  .tabs {
    display: flex;
    gap: 0.4rem;
  }

  .range-buttons button,
  .tabs button {
    background-color: var(--color-shade-3);
    color: inherit;
    border: none;
    border-radius: 6px;
    padding: 0.45rem 0.9rem;
    font-size: 0.9rem;
    cursor: pointer;
  }

  .range-buttons button.active,
  .tabs button.active {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
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
    grid-template-columns: 1fr 16rem;
    gap: 2rem;
    align-items: start;
  }

  .tabs {
    margin-bottom: 1rem;
  }

  .donut-wrap {
    position: relative;
    width: min(280px, 70vw);
    margin: 0 auto 1.5rem;
  }

  .donut {
    width: 100%;
    height: auto;
    display: block;
  }

  :root {
    --donut-track: var(--color-shade-3);
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

  .donut-center .left {
    font-size: 0.85rem;
    opacity: 0.85;
    margin-top: 0.3rem;
  }

  .breakdown {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
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

  @media (max-width: 900px) {
    .layout {
      grid-template-columns: 1fr;
    }

    .filters {
      border-left: none;
      padding-left: 0;
      border-top: 1px solid var(--color-shade-3);
      padding-top: 1rem;
    }
  }
</style>
