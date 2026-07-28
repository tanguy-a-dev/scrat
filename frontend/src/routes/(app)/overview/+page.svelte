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
    const months: { key: string; label: string; income: number; expense: number }[] = [];
    for (let i = MONTHS_SHOWN - 1; i >= 0; i--) {
      const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
      months.push({
        key: `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`,
        label: MONTH_LABELS[d.getMonth()],
        income: 0,
        expense: 0,
      });
    }
    const byKey = new Map(months.map((m) => [m.key, m]));
    for (const t of transactions) {
      const bucket = byKey.get(t.date.slice(0, 7));
      if (!bucket) continue;
      if (t.amount_minor_units < 0) bucket.expense += -t.amount_minor_units;
      else bucket.income += t.amount_minor_units;
    }
    return months;
  });

  /** Average monthly spending across the displayed window — the reference
   * line plotted over the bars. */
  let averageExpenseMinorUnits = $derived(
    monthlyTotals.reduce((sum, m) => sum + m.expense, 0) / (monthlyTotals.length || 1),
  );

  const CHART_WIDTH = 560;
  const CHART_HEIGHT = 220;
  const PADDING_TOP = 12;
  const PADDING_BOTTOM = 26;

  let plotHeight = $derived(CHART_HEIGHT - PADDING_TOP - PADDING_BOTTOM);
  let maxValueMinorUnits = $derived(
    Math.max(1, averageExpenseMinorUnits, ...monthlyTotals.flatMap((m) => [m.income, m.expense])),
  );
  let groupWidth = $derived(CHART_WIDTH / (monthlyTotals.length || 1));
  let barWidth = $derived(groupWidth * 0.28);

  function barHeight(valueMinorUnits: number): number {
    return (valueMinorUnits / maxValueMinorUnits) * plotHeight;
  }

  function barY(valueMinorUnits: number): number {
    return CHART_HEIGHT - PADDING_BOTTOM - barHeight(valueMinorUnits);
  }

  let averageLineY = $derived(CHART_HEIGHT - PADDING_BOTTOM - barHeight(averageExpenseMinorUnits));

  let hasAnyMonthlyActivity = $derived(
    monthlyTotals.some((m) => m.income > 0 || m.expense > 0),
  );
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
        <span class="legend-item"
          ><span class="dot average"></span>Avg. spending: {formatCurrency(
            averageExpenseMinorUnits,
            currency,
          )}</span
        >
      </div>
    </div>

    {#if !hasAnyMonthlyActivity}
      <p class="empty">No transactions in the last {MONTHS_SHOWN} months.</p>
    {:else}
      <svg viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`} class="monthly-chart">
        <line
          x1="0"
          x2={CHART_WIDTH}
          y1={averageLineY}
          y2={averageLineY}
          class="average-line"
        />
        {#each monthlyTotals as month, i (month.key)}
          {@const groupX = i * groupWidth}
          <rect
            x={groupX + groupWidth / 2 - barWidth - 2}
            y={barY(month.income)}
            width={barWidth}
            height={barHeight(month.income)}
            rx="2"
            class="bar income"
          />
          <rect
            x={groupX + groupWidth / 2 + 2}
            y={barY(month.expense)}
            width={barWidth}
            height={barHeight(month.expense)}
            rx="2"
            class="bar expense"
          />
          <text
            x={groupX + groupWidth / 2}
            y={CHART_HEIGHT - 6}
            class="month-label"
            text-anchor="middle">{month.label}</text
          >
        {/each}
      </svg>
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

  .dot.income {
    background-color: var(--color-success);
  }

  .dot.expense {
    background-color: var(--color-danger);
  }

  .dot.average {
    background-color: var(--color-accent);
  }

  .monthly-chart {
    width: 100%;
    height: auto;
    display: block;
  }

  .bar.income {
    fill: var(--color-success);
  }

  .bar.expense {
    fill: var(--color-danger);
  }

  .average-line {
    stroke: var(--color-accent);
    stroke-width: 1.5;
    stroke-dasharray: 5 4;
  }

  .month-label {
    fill: var(--color-text);
    opacity: 0.7;
    font-size: 9px;
  }
</style>
