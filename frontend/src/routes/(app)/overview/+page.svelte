<script lang="ts">
  import { onMount } from "svelte";
  import { api, formatCurrency, type AccountDto, type TransactionDto } from "$lib/api";

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
  let tooltip = $state<{ x: number; y: number; text: string } | null>(null);

  function showTooltip(e: PointerEvent, text: string) {
    const wrap = (e.currentTarget as Element).closest(".chart-wrap") as HTMLElement;
    const rect = wrap.getBoundingClientRect();
    tooltip = { x: e.clientX - rect.left, y: e.clientY - rect.top, text };
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
            {@const incomeLabel = `${month.label} income: ${formatCurrency(month.income, currency)}`}
            {@const expenseLabel = `${month.label} expenses: ${formatCurrency(month.expense, currency)}`}
            <rect
              x={groupX + groupWidth / 2 - barWidth - 2}
              y={barY(month.income)}
              width={barWidth}
              height={barHeight(month.income)}
              rx="2"
              class="bar income"
              role="img"
              aria-label={incomeLabel}
              onpointerenter={(e) => showTooltip(e, incomeLabel)}
              onpointermove={(e) => showTooltip(e, incomeLabel)}
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
              aria-label={expenseLabel}
              onpointerenter={(e) => showTooltip(e, expenseLabel)}
              onpointermove={(e) => showTooltip(e, expenseLabel)}
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
            {@const savingsLabel = `${monthlyTotals[i].label}: ${formatCurrency(p.value, currency)}`}
            <circle
              cx={p.x}
              cy={p.y}
              r="7"
              class="savings-hit"
              role="img"
              aria-label={savingsLabel}
              onpointerenter={(e) => showTooltip(e, savingsLabel)}
              onpointermove={(e) => showTooltip(e, savingsLabel)}
              onpointerleave={hideTooltip}
            />
            <circle cx={p.x} cy={p.y} r="3" class="savings-dot" />
          {/each}
        </svg>

        {#if tooltip}
          <div class="chart-tooltip" style={`left:${tooltip.x}px; top:${tooltip.y}px;`}>
            {tooltip.text}
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
    transform: translate(-50%, -100%);
    margin-top: -10px;
    background-color: var(--color-shade-1);
    color: var(--color-text);
    border: 1px solid var(--color-shade-3);
    border-radius: 6px;
    padding: 0.3rem 0.55rem;
    font-size: 0.75rem;
    white-space: nowrap;
    pointer-events: none;
    z-index: 10;
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
