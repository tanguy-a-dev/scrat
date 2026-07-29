<script lang="ts">
  import { onMount } from "svelte";
  import { scale } from "svelte/transition";
  import { api, formatCurrency, type AccountDto, type TransactionDto } from "$lib/api";

  // Matches .bar.income / .dot.savings / etc in the stylesheet below — kept
  // in sync by hand since the tooltip needs the same colors as swatch values.
  const INCOME_COLOR = "#17b8c4";
  const EXPENSE_COLOR = "var(--color-danger)";
  const SAVINGS_COLOR = "var(--color-accent)";

  const MONTHS_SHOWN = 6;
  const MONTH_LABELS = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];

  let accounts = $state<AccountDto[]>([]);
  let transactions = $state<TransactionDto[]>([]);
  let loading = $state(true);
  let error = $state("");

  onMount(load);

  function isoDate(d: Date): string {
    return d.toISOString().slice(0, 10);
  }

  async function load() {
    loading = true;
    error = "";
    try {
      const now = new Date();
      const start = isoDate(new Date(now.getFullYear(), now.getMonth() - (MONTHS_SHOWN - 1), 1));
      const end = isoDate(new Date(now.getFullYear(), now.getMonth() + 1, 0));
      const [a, t] = await Promise.all([
        api.listAccounts(),
        api.listTransactions(start, end),
      ]);
      accounts = a;
      transactions = t;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  let currency = $derived(accounts[0]?.currency ?? transactions[0]?.currency ?? "EUR");
  let total = $derived(accounts.reduce((sum, a) => sum + a.balance_minor_units, 0));

  /** The last `MONTHS_SHOWN` months (oldest first), zero-filled so a month
   * with no transactions still shows up as an empty bar rather than a gap. */
  let monthlyTotals = $derived.by(() => {
    const now = new Date();
    const months: {
      key: string;
      label: string;
      income: number;
      expense: number;
      savings: number;
    }[] = [];
    for (let i = MONTHS_SHOWN - 1; i >= 0; i--) {
      const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
      months.push({
        key: `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`,
        label: MONTH_LABELS[d.getMonth()],
        income: 0,
        expense: 0,
        savings: 0,
      });
    }
    const byKey = new Map(months.map((m) => [m.key, m]));
    for (const t of transactions) {
      const bucket = byKey.get(t.date.slice(0, 7));
      if (!bucket) continue;
      if (t.amount_minor_units < 0) bucket.expense += -t.amount_minor_units;
      else bucket.income += t.amount_minor_units;
    }
    for (const m of months) m.savings = m.income - m.expense;
    return months;
  });

  const CHART_WIDTH = 560;
  const CHART_HEIGHT = 220;
  const PADDING_TOP = 12;
  const PADDING_BOTTOM = 26;

  let plotHeight = $derived(CHART_HEIGHT - PADDING_TOP - PADDING_BOTTOM);
  let maxValueMinorUnits = $derived(
    Math.max(1, ...monthlyTotals.flatMap((m) => [m.income, m.expense])),
  );
  let groupWidth = $derived(CHART_WIDTH / (monthlyTotals.length || 1));
  let barWidth = $derived(groupWidth * 0.28);

  function barHeight(valueMinorUnits: number): number {
    return (valueMinorUnits / maxValueMinorUnits) * plotHeight;
  }

  function barY(valueMinorUnits: number): number {
    return CHART_HEIGHT - PADDING_BOTTOM - barHeight(valueMinorUnits);
  }

  /** Point for a signed value (savings can go negative). Uses the same
   * baseline/scale as the bars, but clamped so the line never dips below the
   * zero baseline visually — a negative month still reads as flat at zero,
   * with the real (possibly negative) figure available on hover. */
  function pointY(valueMinorUnits: number): number {
    const baseline = CHART_HEIGHT - PADDING_BOTTOM;
    const raw = baseline - (valueMinorUnits / maxValueMinorUnits) * plotHeight;
    return Math.min(raw, baseline);
  }

  let savingsPoints = $derived(
    monthlyTotals.map((m, i) => ({
      x: i * groupWidth + groupWidth / 2,
      y: pointY(m.savings),
      value: m.savings,
    })),
  );

  let savingsLinePoints = $derived(savingsPoints.map((p) => `${p.x},${p.y}`).join(" "));

  let hasAnyMonthlyActivity = $derived(
    monthlyTotals.some((m) => m.income > 0 || m.expense > 0),
  );

  // Custom tooltip driven by pointer events instead of the native SVG
  // <title> (which has a built-in hover delay before it appears).
  const TOOLTIP_MARGIN = 55;
  const TOOLTIP_FLIP_THRESHOLD = 40;

  let tooltip = $state<{
    x: number;
    y: number;
    below: boolean;
    month: string;
    seriesLabel: string;
    amount: string;
    color: string;
  } | null>(null);

  function showTooltip(
    e: PointerEvent,
    month: string,
    seriesLabel: string,
    amount: string,
    color: string,
  ) {
    const wrap = (e.currentTarget as Element).closest(".chart-wrap") as HTMLElement;
    const rect = wrap.getBoundingClientRect();
    const rawX = e.clientX - rect.left;
    const rawY = e.clientY - rect.top;
    tooltip = {
      x: Math.min(Math.max(rawX, TOOLTIP_MARGIN), rect.width - TOOLTIP_MARGIN),
      y: rawY,
      below: rawY < TOOLTIP_FLIP_THRESHOLD,
      month,
      seriesLabel,
      amount,
      color,
    };
  }

  function hideTooltip() {
    tooltip = null;
  }
</script>

<h1>Overview</h1>

{#if loading}
  <p>Loading…</p>
{:else if error}
  <p class="error">{error}</p>
{:else if accounts.length === 0}
  <p class="empty">
    No accounts yet. Head to <a href="/accounts">Accounts</a> to add one.
  </p>
{:else}
  <div class="grid">
    <div class="box total">
      <span class="label">Total available</span>
      <span class="amount">{formatCurrency(total, currency)}</span>
    </div>
    {#each accounts as account (account.id)}
      <div class="box">
        <span class="label">{account.name}</span>
        <span class="amount"
          >{formatCurrency(account.balance_minor_units, account.currency)}</span
        >
      </div>
    {/each}
  </div>

  <div class="chart-card">
    <div class="chart-header">
      <h2>Income &amp; expenses by month</h2>
      <div class="legend">
        <span class="legend-item"><span class="dot income"></span>Income</span>
        <span class="legend-item"><span class="dot expense"></span>Expenses</span>
        <span class="legend-item"><span class="dot savings"></span>Savings</span>
      </div>
    </div>

    {#if !hasAnyMonthlyActivity}
      <p class="empty">No transactions in the last {MONTHS_SHOWN} months.</p>
    {:else}
      <div class="chart-wrap">
        <svg viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`} class="monthly-chart">
          {#each monthlyTotals as month, i (month.key)}
            {@const groupX = i * groupWidth}
            {@const incomeText = formatCurrency(month.income, currency)}
            {@const expenseText = formatCurrency(month.expense, currency)}
            <rect
              x={groupX + groupWidth / 2 - barWidth - 2}
              y={barY(month.income)}
              width={barWidth}
              height={barHeight(month.income)}
              rx="2"
              class="bar income"
              role="img"
              aria-label={`${month.label} income: ${incomeText}`}
              onpointerenter={(e) => showTooltip(e, month.label, "Income", incomeText, INCOME_COLOR)}
              onpointermove={(e) => showTooltip(e, month.label, "Income", incomeText, INCOME_COLOR)}
              onpointerleave={hideTooltip}
            />
            <rect
              x={groupX + groupWidth / 2 + 2}
              y={barY(month.expense)}
              width={barWidth}
              height={barHeight(month.expense)}
              rx="2"
              class="bar expense"
              role="img"
              aria-label={`${month.label} expenses: ${expenseText}`}
              onpointerenter={(e) => showTooltip(e, month.label, "Expenses", expenseText, EXPENSE_COLOR)}
              onpointermove={(e) => showTooltip(e, month.label, "Expenses", expenseText, EXPENSE_COLOR)}
              onpointerleave={hideTooltip}
            />
            <text
              x={groupX + groupWidth / 2}
              y={CHART_HEIGHT - 6}
              class="month-label"
              text-anchor="middle">{month.label}</text
            >
          {/each}

          <polyline points={savingsLinePoints} class="savings-line" />
          {#each savingsPoints as p, i (monthlyTotals[i].key)}
            {@const savingsText = formatCurrency(p.value, currency)}
            <circle
              cx={p.x}
              cy={p.y}
              r="7"
              class="savings-hit"
              role="img"
              aria-label={`${monthlyTotals[i].label} savings: ${savingsText}`}
              onpointerenter={(e) => showTooltip(e, monthlyTotals[i].label, "Savings", savingsText, SAVINGS_COLOR)}
              onpointermove={(e) => showTooltip(e, monthlyTotals[i].label, "Savings", savingsText, SAVINGS_COLOR)}
              onpointerleave={hideTooltip}
            />
            <circle cx={p.x} cy={p.y} r="3" class="savings-dot" />
          {/each}
        </svg>

        {#if tooltip}
          <div
            class="chart-tooltip"
            class:below={tooltip.below}
            style={`left:${tooltip.x}px; top:${tooltip.y}px;`}
            transition:scale={{ duration: 90, start: 0.9 }}
          >
            <div class="tooltip-month">{tooltip.month}</div>
            <div class="tooltip-row">
              <span class="tooltip-dot" style={`background-color:${tooltip.color}`}></span>
              <span class="tooltip-series">{tooltip.seriesLabel}</span>
              <span class="tooltip-amount">{tooltip.amount}</span>
            </div>
          </div>
        {/if}
      </div>
    {/if}
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

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
    gap: 1rem;
  }

  .box {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 1.25rem;
    border-radius: 10px;
    background-color: var(--color-box);
  }

  .box.total {
    background-color: var(--color-box-accent-bg);
    color: var(--color-box-accent-text);
  }

  .label {
    font-size: 0.85rem;
    opacity: 0.8;
  }

  .amount {
    font-size: 1.5rem;
    font-weight: 700;
  }

  .chart-card {
    margin-top: 1.5rem;
    padding: 1.25rem;
    border-radius: 10px;
    background-color: var(--color-box);
  }

  .chart-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-bottom: 0.75rem;
  }

  .chart-header h2 {
    font-size: 1rem;
    margin: 0;
  }

  .legend {
    display: flex;
    gap: 1rem;
    font-size: 0.8rem;
    opacity: 0.85;
  }

  .legend-item {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }

  .dot {
    width: 0.6rem;
    height: 0.6rem;
    border-radius: 50%;
    flex-shrink: 0;
  }

  /* A distinct cyan shade from --color-accent, so the Income bars don't
     blend visually with the savings line/dots drawn over them. */
  .dot.income {
    background-color: #17b8c4;
  }

  .dot.expense {
    background-color: var(--color-danger);
  }

  .dot.savings {
    background-color: var(--color-accent);
  }

  .chart-wrap {
    position: relative;
  }

  .monthly-chart {
    width: 100%;
    height: auto;
    display: block;
  }

  .chart-tooltip {
    position: absolute;
    transform: translate(-50%, calc(-100% - 10px));
    background-color: var(--color-shade-1);
    color: var(--color-text);
    border: 1px solid var(--color-shade-3);
    border-radius: 6px;
    padding: 0.4rem 0.6rem;
    font-size: 0.75rem;
    white-space: nowrap;
    pointer-events: none;
    z-index: 10;
  }

  .chart-tooltip.below {
    transform: translate(-50%, 10px);
  }

  .tooltip-month {
    font-size: 0.7rem;
    opacity: 0.7;
    text-transform: uppercase;
    margin-bottom: 0.2rem;
  }

  .tooltip-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .tooltip-dot {
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .tooltip-series {
    opacity: 0.85;
  }

  .tooltip-amount {
    font-weight: 700;
  }

  .bar {
    transform-box: fill-box;
    transform-origin: 50% 100%;
    transition: transform 0.15s ease;
  }

  .bar:hover {
    transform: scale(1.08);
  }

  .bar.income {
    fill: #17b8c4;
  }

  .bar.expense {
    fill: var(--color-danger);
  }

  .savings-line {
    fill: none;
    stroke: var(--color-accent);
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  .savings-hit {
    fill: transparent;
    pointer-events: all;
    cursor: default;
  }

  .savings-dot {
    fill: var(--color-accent);
    pointer-events: none;
    transform-box: fill-box;
    transform-origin: center;
    transition: transform 0.15s ease;
  }

  .savings-hit:hover + .savings-dot {
    transform: scale(1.6);
  }

  .month-label {
    fill: var(--color-text);
    opacity: 0.7;
    font-size: 9px;
  }
</style>
