<script lang="ts">
  import { onMount } from "svelte";
  import { scale } from "svelte/transition";
  import {
    api,
    countsTowardTotals,
    formatCurrency,
    formatCurrencyRounded,
    type AccountDto,
    type RecurringChargeDto,
    type TransactionDto,
  } from "$lib/api";

  // Matches .bar.income / .dot.savings / etc in the stylesheet below — kept
  // in sync by hand since the tooltip needs the same colors as swatch values.
  const INCOME_COLOR = "#17b8c4";
  const EXPENSE_COLOR = "var(--color-danger)";
  const SAVINGS_COLOR = "var(--color-accent)";
  const BALANCE_COLOR = "var(--color-accent)";

  const MONTHS_SHOWN = 6;
  /** Balance-over-time reaches further back than the bar chart: a level reads
   * better with more points, and the extra months are free — both charts are
   * fed by the same single fetch. */
  const BALANCE_MONTHS_SHOWN = 12;
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
  let recurring = $state<RecurringChargeDto[]>([]);
  let loading = $state(true);
  let error = $state("");

  // Captured once, so every derived figure on the page is anchored to the same
  // "now" rather than each recomputing its own.
  const today = new Date();
  const dayOfMonth = today.getDate();

  function pad2(n: number): string {
    return String(n).padStart(2, "0");
  }

  /** Local-calendar YYYY-MM-DD. Deliberately not `toISOString()`, which
   * converts to UTC first and so rolls a local midnight back to the previous
   * day anywhere east of Greenwich. */
  function isoDate(d: Date): string {
    return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
  }

  function monthKey(d: Date): string {
    return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}`;
  }

  const currentMonthKey = monthKey(today);
  const previousMonthKey = monthKey(new Date(today.getFullYear(), today.getMonth() - 1, 1));

  onMount(load);

  async function load() {
    loading = true;
    error = "";
    try {
      const start = isoDate(
        new Date(today.getFullYear(), today.getMonth() - (BALANCE_MONTHS_SHOWN - 1), 1),
      );
      // Open-ended on purpose. A transaction dated in the future is already
      // counted in account.balance_minor_units (the backend sums the whole
      // ledger with no date filter), so it has to be in the fetched set too or
      // every reconstructed past balance is off by it. Months outside a chart's
      // window are ignored when bucketing.
      const end = "9999-12-31";
      // Recurring detection has its own (much longer) lookback, decided
      // backend-side — it needs three occurrences of a charge, which a yearly
      // one can only reach across years.
      const [a, t, r] = await Promise.all([
        api.listAccounts(),
        api.listTransactions(start, end),
        api.listRecurringCharges(),
      ]);
      accounts = a;
      transactions = t;
      recurring = r;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  let currency = $derived(accounts[0]?.currency ?? transactions[0]?.currency ?? "EUR");
  let total = $derived(accounts.reduce((sum, a) => sum + a.balance_minor_units, 0));

  /** Transactions that are real income or spending. Transfers between the
   * user's own accounts and reconciliation adjustments are excluded: a
   * transfer would otherwise land as an expense on one account and income on
   * the other, inflating both bars of every month it appears in even though
   * savings nets out to the right number.
   *
   * Balance figures deliberately keep using the unfiltered `transactions` —
   * the money really did move, so it counts there. */
  let reportableTransactions = $derived(transactions.filter(countsTowardTotals));

  /** The last `monthsCount` months (oldest first), zero-filled so a month with
   * no transactions still shows up as an empty bar rather than a gap. */
  function buildMonthlyTotals(monthsCount: number) {
    const months: {
      key: string;
      label: string;
      income: number;
      expense: number;
      savings: number;
    }[] = [];
    for (let i = monthsCount - 1; i >= 0; i--) {
      const d = new Date(today.getFullYear(), today.getMonth() - i, 1);
      months.push({
        key: monthKey(d),
        label: MONTH_LABELS[d.getMonth()],
        income: 0,
        expense: 0,
        savings: 0,
      });
    }
    const byKey = new Map(months.map((m) => [m.key, m]));
    for (const t of reportableTransactions) {
      const bucket = byKey.get(t.date.slice(0, 7));
      if (!bucket) continue;
      if (t.amount_minor_units < 0) bucket.expense += -t.amount_minor_units;
      else bucket.income += t.amount_minor_units;
    }
    for (const m of months) m.savings = m.income - m.expense;
    return months;
  }

  let monthlyTotals = $derived.by(() => buildMonthlyTotals(MONTHS_SHOWN));

  // ---------------------------------------------------------------------------
  // Per-account movement this month — the "since the 1st" figure on each tile.
  // ---------------------------------------------------------------------------

  let currentMonthDeltaByAccount = $derived.by(() => {
    const byAccount = new Map<string, number>();
    for (const t of transactions) {
      if (t.date.slice(0, 7) !== currentMonthKey) continue;
      byAccount.set(t.account_id, (byAccount.get(t.account_id) ?? 0) + t.amount_minor_units);
    }
    return byAccount;
  });

  // Summed over the accounts rather than over the whole map, so it stays
  // consistent with `total` (which is also account-derived).
  let totalCurrentMonthDelta = $derived(
    accounts.reduce((sum, a) => sum + (currentMonthDeltaByAccount.get(a.id) ?? 0), 0),
  );

  // ---------------------------------------------------------------------------
  // "This month" strip.
  // ---------------------------------------------------------------------------

  let spentThisMonth = $derived(
    reportableTransactions.reduce(
      (sum, t) =>
        t.amount_minor_units < 0 && t.date.slice(0, 7) === currentMonthKey
          ? sum - t.amount_minor_units
          : sum,
      0,
    ),
  );

  /** Last month's spending counted only up to the same day-of-month, so a
   * comparison made on the 3rd isn't measured against a whole month. Days past
   * the end of a shorter month simply contribute nothing. */
  let spentLastMonthToDate = $derived(
    reportableTransactions.reduce((sum, t) => {
      if (t.amount_minor_units >= 0) return sum;
      if (t.date.slice(0, 7) !== previousMonthKey) return sum;
      if (Number(t.date.slice(8, 10)) > dayOfMonth) return sum;
      return sum - t.amount_minor_units;
    }, 0),
  );

  let spendDelta = $derived(spentThisMonth - spentLastMonthToDate);
  let spendDeltaPercent = $derived(
    spentLastMonthToDate > 0 ? Math.round((spendDelta / spentLastMonthToDate) * 100) : null,
  );

  /** Mean expense/savings over the full `MONTHS_SHOWN`-month window — the same
   * months the bar chart plots, current partial month included. */
  let meanSpend = $derived(
    Math.round(monthlyTotals.reduce((sum, m) => sum + m.expense, 0) / monthlyTotals.length),
  );

  let meanSavings = $derived(
    Math.round(monthlyTotals.reduce((sum, m) => sum + m.savings, 0) / monthlyTotals.length),
  );

  // ---------------------------------------------------------------------------
  // Recurring commitments.
  // ---------------------------------------------------------------------------

  /** How many active charges to show before collapsing the rest behind a
   * toggle. Enough to be useful at a glance without the panel taking over the
   * page for someone with thirty subscriptions. */
  const RECURRING_PREVIEW_COUNT = 6;

  let showAllRecurring = $state(false);

  let activeRecurring = $derived(recurring.filter((c) => c.is_active));
  let lapsedRecurring = $derived(recurring.filter((c) => !c.is_active));

  let visibleRecurring = $derived(
    showAllRecurring ? activeRecurring : activeRecurring.slice(0, RECURRING_PREVIEW_COUNT),
  );

  /** Total monthly commitment. Active charges only — money that stopped
   * leaving isn't committed, however regular it once was. */
  let monthlyCommitment = $derived(
    activeRecurring.reduce((sum, c) => sum + c.monthly_equivalent_minor_units, 0),
  );

  let recurringCurrency = $derived(recurring[0]?.currency ?? currency);

  /** "12 Aug". Parsed by splitting the ISO string rather than via `new Date`,
   * which reads a bare YYYY-MM-DD as UTC midnight and so renders the previous
   * day for anyone west of Greenwich. */
  function formatShortDate(iso: string): string {
    const [, month, day] = iso.split("-");
    const label = MONTH_LABELS[Number(month) - 1];
    return label ? `${Number(day)} ${label}` : iso;
  }

  // ---------------------------------------------------------------------------
  // Shared chart scaffolding.
  // ---------------------------------------------------------------------------

  const CHART_WIDTH = 560;
  const CHART_HEIGHT = 220;
  const PADDING_TOP = 12;
  const PADDING_BOTTOM = 26;
  const PADDING_LEFT = 58;
  const PADDING_RIGHT = 10;
  const AXIS_TICK_COUNT = 4;

  const plotHeight = CHART_HEIGHT - PADDING_TOP - PADDING_BOTTOM;
  const plotBottom = CHART_HEIGHT - PADDING_BOTTOM;

  /** Rounds a raw axis step up to a "nice" 1/2/5 × 10^n value, so the scale
   * reads as round numbers (e.g. 500, 1000) rather than awkward fractions. */
  function niceStep(rawStep: number): number {
    if (rawStep <= 0) return 1;
    const magnitude = 10 ** Math.floor(Math.log10(rawStep));
    const residual = rawStep / magnitude;
    const niceResidual = residual <= 1 ? 1 : residual <= 2 ? 2 : residual <= 5 ? 5 : 10;
    return niceResidual * magnitude;
  }

  type Scale = { min: number; max: number; ticks: number[] };

  /** A nice-rounded [min, max] domain covering `values`, in minor units.
   * `includeZero` forces the baseline into the domain — needed by the bar
   * chart (bars grow from zero) and by any series that crosses into negative. */
  function buildScale(values: number[], includeZero: boolean): Scale {
    if (values.length === 0) return { min: 0, max: 1, ticks: [0, 1] };
    const rawMax = Math.max(...values, ...(includeZero ? [0] : []));
    const rawMin = Math.min(...values, ...(includeZero ? [0] : []));
    // A step below one minor unit is meaningless — amounts are integers.
    const step = Math.max(1, niceStep(Math.max(rawMax - rawMin, 1) / AXIS_TICK_COUNT));
    const min = Math.floor(rawMin / step) * step;
    const max = Math.ceil(rawMax / step) * step;
    const ticks: number[] = [];
    for (let v = min; v <= max + step * 0.001; v += step) ticks.push(Math.round(v));
    return { min, max, ticks };
  }

  function scaleY(value: number, scaleDef: Scale): number {
    const span = scaleDef.max - scaleDef.min || 1;
    return plotBottom - ((value - scaleDef.min) / span) * plotHeight;
  }

  // ---------------------------------------------------------------------------
  // Income & expenses chart.
  // ---------------------------------------------------------------------------

  /** One domain shared by income, expenses and savings, so all three read off
   * the same axis. Savings is part of the extent rather than clamped at zero —
   * an overspent month has to be able to draw below the baseline, otherwise it
   * looks identical to breaking exactly even. */
  let monthlyScale = $derived(
    buildScale(
      monthlyTotals.flatMap((m) => [m.income, m.expense, m.savings]),
      true,
    ),
  );

  let monthlyZeroY = $derived(scaleY(0, monthlyScale));
  let groupWidth = $derived((CHART_WIDTH - PADDING_LEFT) / (monthlyTotals.length || 1));
  let barWidth = $derived(groupWidth * 0.28);

  function barY(valueMinorUnits: number): number {
    return scaleY(valueMinorUnits, monthlyScale);
  }

  function barHeight(valueMinorUnits: number): number {
    return Math.max(0, monthlyZeroY - scaleY(valueMinorUnits, monthlyScale));
  }

  let savingsPoints = $derived(
    monthlyTotals.map((m, i) => ({
      x: PADDING_LEFT + i * groupWidth + groupWidth / 2,
      y: scaleY(m.savings, monthlyScale),
      value: m.savings,
    })),
  );

  let savingsLinePoints = $derived(savingsPoints.map((p) => `${p.x},${p.y}`).join(" "));

  let hasAnyMonthlyActivity = $derived(monthlyTotals.some((m) => m.income > 0 || m.expense > 0));

  // ---------------------------------------------------------------------------
  // Balance over time.
  // ---------------------------------------------------------------------------

  /** Balance at the end of each month in the window, reconstructed backwards
   * from today's known total by undoing each month's net movement.
   *
   * This is exact, not an estimate: an account's balance is
   * `opening_balance + SUM(every transaction)`, so the balance at the end of
   * month M is today's total minus everything dated after M — and everything
   * dated after M is inside the fetched window by construction. */
  let balanceSeries = $derived.by(() => {
    if (accounts.length === 0 || transactions.length === 0) return [];

    const months: { key: string; label: string; year: number; balance: number }[] = [];
    for (let i = BALANCE_MONTHS_SHOWN - 1; i >= 0; i--) {
      const d = new Date(today.getFullYear(), today.getMonth() - i, 1);
      months.push({
        key: monthKey(d),
        label: MONTH_LABELS[d.getMonth()],
        year: d.getFullYear(),
        balance: 0,
      });
    }

    const newestKey = months[months.length - 1].key;
    const netByMonth = new Map<string, number>();
    let afterWindow = 0;
    let firstKey: string | null = null;
    for (const t of transactions) {
      const key = t.date.slice(0, 7);
      if (firstKey === null || key < firstKey) firstKey = key;
      if (key > newestKey) afterWindow += t.amount_minor_units;
      else netByMonth.set(key, (netByMonth.get(key) ?? 0) + t.amount_minor_units);
    }

    let running = total - afterWindow;
    for (let i = months.length - 1; i >= 0; i--) {
      months[i].balance = running;
      running -= netByMonth.get(months[i].key) ?? 0;
    }

    // Drop months that predate all history. They'd render as a long flat run at
    // the opening balance — true, but it says nothing while squashing the part
    // of the line that does. One month is kept ahead of the first activity so
    // the first real move has something to move from.
    const first = firstKey;
    const firstIndex = first === null ? 0 : months.findIndex((m) => m.key >= first);
    return months.slice(Math.max(0, firstIndex <= 0 ? 0 : firstIndex - 1));
  });

  /** Unlike the bar chart, this axis is not pinned to zero — a balance is a
   * level, not a magnitude, and zero-anchoring flattens the variation that is
   * the whole point of the chart. Zero is forced in only when the line actually
   * crosses it, so an overdraft still reads as one. */
  let balanceScale = $derived(
    buildScale(
      balanceSeries.map((m) => m.balance),
      balanceSeries.some((m) => m.balance < 0),
    ),
  );

  let balanceStepX = $derived(
    (CHART_WIDTH - PADDING_LEFT - PADDING_RIGHT) / Math.max(1, balanceSeries.length - 1),
  );

  let balancePoints = $derived(
    balanceSeries.map((m, i) => ({
      key: m.key,
      label: m.label,
      year: m.year,
      balance: m.balance,
      x: PADDING_LEFT + i * balanceStepX,
      y: scaleY(m.balance, balanceScale),
    })),
  );

  let balanceLinePoints = $derived(balancePoints.map((p) => `${p.x},${p.y}`).join(" "));

  let balanceAreaPath = $derived.by(() => {
    if (balancePoints.length < 2) return "";
    const line = balancePoints.map((p, i) => `${i === 0 ? "M" : "L"}${p.x},${p.y}`).join(" ");
    const first = balancePoints[0];
    const last = balancePoints[balancePoints.length - 1];
    return `${line} L${last.x},${plotBottom} L${first.x},${plotBottom} Z`;
  });

  let hasBalanceHistory = $derived(balancePoints.length >= 2);

  /** Change across the whole charted window — the headline for this card. */
  let balanceChange = $derived(
    hasBalanceHistory
      ? balancePoints[balancePoints.length - 1].balance - balancePoints[0].balance
      : 0,
  );

  // ---------------------------------------------------------------------------
  // Tooltip, shared by both charts.
  // ---------------------------------------------------------------------------

  // Custom tooltip driven by pointer events instead of the native SVG
  // <title> (which has a built-in hover delay before it appears).
  const TOOLTIP_MARGIN = 55;
  const TOOLTIP_FLIP_THRESHOLD = 40;

  let tooltip = $state<{
    chart: "monthly" | "balance";
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
    chart: "monthly" | "balance",
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
      chart,
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

{#snippet chartTooltip(t: NonNullable<typeof tooltip>)}
  <div
    class="chart-tooltip"
    class:below={t.below}
    style={`left:${t.x}px; top:${t.y}px;`}
    transition:scale={{ duration: 90, start: 0.9 }}
  >
    <div class="tooltip-month">{t.month}</div>
    <div class="tooltip-row">
      <span class="tooltip-dot" style={`background-color:${t.color}`}></span>
      <span class="tooltip-series">{t.seriesLabel}</span>
      <span class="tooltip-amount">{t.amount}</span>
    </div>
  </div>
{/snippet}

{#snippet monthDelta(minorUnits: number, currencyCode: string)}
  {#if minorUnits === 0}
    <span class="delta flat">No movement this month</span>
  {:else}
    <span class="delta" class:up={minorUnits > 0} class:down={minorUnits < 0}>
      {minorUnits > 0 ? "↑" : "↓"}
      {formatCurrency(Math.abs(minorUnits), currencyCode)} this month
    </span>
  {/if}
{/snippet}

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
      {@render monthDelta(totalCurrentMonthDelta, currency)}
    </div>
    {#each accounts as account (account.id)}
      <div class="box">
        <span class="label">{account.name}</span>
        <span class="amount">{formatCurrency(account.balance_minor_units, account.currency)}</span>
        {@render monthDelta(currentMonthDeltaByAccount.get(account.id) ?? 0, account.currency)}
      </div>
    {/each}
  </div>

  <div class="stats-row">
    <div class="month-card">
      <h2>This month</h2>
      <div class="month-stats">
        <div class="stat">
          <span class="label">Spent so far</span>
          <span class="stat-amount">{formatCurrency(spentThisMonth, currency)}</span>
          <span class="hint">Since the 1st</span>
        </div>

        <div class="stat">
          <span class="label">vs. same point last month</span>
          {#if spentLastMonthToDate === 0 && spentThisMonth === 0}
            <span class="stat-amount muted">—</span>
            <span class="hint">Nothing spent in either month yet</span>
          {:else}
            <span class="stat-amount" class:over={spendDelta > 0} class:under={spendDelta < 0}>
              {spendDelta > 0 ? "↑ " : spendDelta < 0 ? "↓ " : ""}{formatCurrency(
                Math.abs(spendDelta),
                currency,
              )}
              {spendDelta > 0 ? "more" : spendDelta < 0 ? "less" : "identical"}
            </span>
            <span class="hint">
              {formatCurrency(spentLastMonthToDate, currency)} by day {dayOfMonth} last month{spendDeltaPercent ===
              null
                ? ""
                : ` · ${spendDeltaPercent > 0 ? "+" : ""}${spendDeltaPercent}%`}
            </span>
          {/if}
        </div>
      </div>
    </div>

    <div class="month-card">
      <h2>Mean monthly spend</h2>
      <div class="month-stats">
        <div class="stat">
          <span class="stat-amount">{formatCurrency(meanSpend, currency)}</span>
          <span class="hint">Over the last {MONTHS_SHOWN} months</span>
        </div>
      </div>
    </div>

    <div class="month-card">
      <h2>Mean savings</h2>
      <div class="month-stats">
        <div class="stat">
          <span class="stat-amount" class:over={meanSavings < 0} class:under={meanSavings >= 0}>
            {formatCurrency(meanSavings, currency)}
          </span>
          <span class="hint">Over the last {MONTHS_SHOWN} months</span>
        </div>
      </div>
    </div>
  </div>

  <div class="charts-row">
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
            {#each monthlyScale.ticks as tick (tick)}
              {@const y = scaleY(tick, monthlyScale)}
              <line
                x1={PADDING_LEFT}
                x2={CHART_WIDTH}
                y1={y}
                y2={y}
                class="axis-gridline"
                class:zero={tick === 0}
              />
              <text x={PADDING_LEFT - 8} y={y} class="axis-label" text-anchor="end"
                >{formatCurrencyRounded(tick, currency)}</text
              >
            {/each}
  
            {#each monthlyTotals as month, i (month.key)}
              {@const groupX = PADDING_LEFT + i * groupWidth}
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
                onpointerenter={(e) =>
                  showTooltip(e, "monthly", month.label, "Income", incomeText, INCOME_COLOR)}
                onpointermove={(e) =>
                  showTooltip(e, "monthly", month.label, "Income", incomeText, INCOME_COLOR)}
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
                onpointerenter={(e) =>
                  showTooltip(e, "monthly", month.label, "Expenses", expenseText, EXPENSE_COLOR)}
                onpointermove={(e) =>
                  showTooltip(e, "monthly", month.label, "Expenses", expenseText, EXPENSE_COLOR)}
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
              {@const savingsMonth = monthlyTotals[i].label}
              <circle
                cx={p.x}
                cy={p.y}
                r="7"
                class="savings-hit"
                role="img"
                aria-label={`${savingsMonth} savings: ${savingsText}`}
                onpointerenter={(e) =>
                  showTooltip(e, "monthly", savingsMonth, "Savings", savingsText, SAVINGS_COLOR)}
                onpointermove={(e) =>
                  showTooltip(e, "monthly", savingsMonth, "Savings", savingsText, SAVINGS_COLOR)}
                onpointerleave={hideTooltip}
              />
              <circle cx={p.x} cy={p.y} r="3" class="savings-dot" />
            {/each}
          </svg>
  
          {#if tooltip && tooltip.chart === "monthly"}
            {@render chartTooltip(tooltip)}
          {/if}
        </div>
      {/if}
    </div>
  
    <div class="chart-card">
      <div class="chart-header">
        <h2>Balance over time</h2>
        {#if hasBalanceHistory}
          <span class="chart-note" class:up={balanceChange > 0} class:down={balanceChange < 0}>
            {balanceChange > 0 ? "↑ " : balanceChange < 0 ? "↓ " : ""}{formatCurrency(
              Math.abs(balanceChange),
              currency,
            )} over {balancePoints.length} months
          </span>
        {/if}
      </div>
  
      {#if !hasBalanceHistory}
        <p class="empty">Not enough history yet — balance over time needs at least two months.</p>
      {:else}
        <div class="chart-wrap">
          <svg viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`} class="monthly-chart">
            {#each balanceScale.ticks as tick (tick)}
              {@const y = scaleY(tick, balanceScale)}
              <line
                x1={PADDING_LEFT}
                x2={CHART_WIDTH}
                y1={y}
                y2={y}
                class="axis-gridline"
                class:zero={tick === 0}
              />
              <text x={PADDING_LEFT - 8} y={y} class="axis-label" text-anchor="end"
                >{formatCurrencyRounded(tick, currency)}</text
              >
            {/each}
  
            <path d={balanceAreaPath} class="balance-area" />
            <polyline points={balanceLinePoints} class="balance-line" />
  
            {#each balancePoints as p (p.key)}
              {@const balanceText = formatCurrency(p.balance, currency)}
              {@const balanceMonth = `${p.label} ${p.year}`}
              <rect
                x={p.x - balanceStepX / 2}
                y={PADDING_TOP}
                width={balanceStepX}
                height={plotHeight}
                class="column-hit"
                role="img"
                aria-label={`End of ${balanceMonth}: ${balanceText}`}
                onpointerenter={(e) =>
                  showTooltip(e, "balance", balanceMonth, "Balance", balanceText, BALANCE_COLOR)}
                onpointermove={(e) =>
                  showTooltip(e, "balance", balanceMonth, "Balance", balanceText, BALANCE_COLOR)}
                onpointerleave={hideTooltip}
              />
              <circle cx={p.x} cy={p.y} r="3" class="balance-dot" />
            {/each}
  
            {#each balancePoints as p (p.key)}
              <text x={p.x} y={CHART_HEIGHT - 6} class="month-label" text-anchor="middle"
                >{p.label}</text
              >
            {/each}
          </svg>
  
          {#if tooltip && tooltip.chart === "balance"}
            {@render chartTooltip(tooltip)}
          {/if}
        </div>
      {/if}
    </div>
  </div>

  <div class="recurring-card">
    <div class="chart-header">
      <h2>Recurring commitments</h2>
      {#if activeRecurring.length > 0}
        <span class="chart-note">
          {formatCurrency(monthlyCommitment, recurringCurrency)} / month across
          {activeRecurring.length}
          {activeRecurring.length === 1 ? "charge" : "charges"}
        </span>
      {/if}
    </div>

    {#if recurring.length === 0}
      <p class="empty">
        Nothing detected yet. A charge has to appear at least three times, on a steady rhythm and
        for about the same amount, before it counts as recurring.
      </p>
    {:else}
      <ul class="recurring-list">
        {#each visibleRecurring as charge (charge.label + charge.first_seen)}
          <li class="recurring-row">
            <span class="recurring-label" title={charge.label}>{charge.label}</span>
            <span class="cadence-badge">{charge.cadence}</span>
            <span class="recurring-amount">
              {formatCurrency(charge.typical_amount_minor_units, charge.currency)}
            </span>
            <span class="recurring-meta">
              {#if charge.cadence === "monthly"}
                Next {formatShortDate(charge.next_expected)}
              {:else}
                ≈ {formatCurrency(charge.monthly_equivalent_minor_units, charge.currency)}/mo ·
                next {formatShortDate(charge.next_expected)}
              {/if}
            </span>
          </li>
        {/each}
      </ul>

      {#if activeRecurring.length > RECURRING_PREVIEW_COUNT}
        <button class="link-button" onclick={() => (showAllRecurring = !showAllRecurring)}>
          {showAllRecurring
            ? "Show fewer"
            : `Show all ${activeRecurring.length} recurring charges`}
        </button>
      {/if}

      {#if lapsedRecurring.length > 0}
        <div class="lapsed">
          <h3>Not seen recently</h3>
          <p class="hint">
            These billed on a rhythm and then stopped. Either they were cancelled, or a payment
            failed and is worth checking.
          </p>
          <ul class="recurring-list">
            {#each lapsedRecurring as charge (charge.label + charge.first_seen)}
              <li class="recurring-row lapsed-row">
                <span class="recurring-label" title={charge.label}>{charge.label}</span>
                <span class="cadence-badge">{charge.cadence}</span>
                <span class="recurring-amount">
                  {formatCurrency(charge.typical_amount_minor_units, charge.currency)}
                </span>
                <span class="recurring-meta">Last seen {formatShortDate(charge.last_seen)}</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
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

  .empty a {
    color: var(--color-accent);
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

  .delta {
    font-size: 0.78rem;
    opacity: 0.9;
  }

  .delta.flat {
    opacity: 0.6;
  }

  .delta.up {
    color: var(--color-accent);
  }

  .delta.down {
    color: var(--color-danger);
  }

  /* The total tile is filled with the accent color, so accent-on-accent text
     would vanish — inherit the tile's own contrast color there instead. */
  .box.total .delta,
  .box.total .delta.up,
  .box.total .delta.down {
    color: inherit;
    opacity: 0.75;
  }

  .month-card,
  .chart-card,
  .recurring-card {
    margin-top: 1.5rem;
    padding: 1.25rem;
    border-radius: 10px;
    background-color: var(--color-box);
  }

  .recurring-card h2 {
    font-size: 1rem;
    margin: 0;
  }

  .recurring-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  /* Label takes the slack; the other three columns size to their content so
     amounts stay aligned down the list regardless of merchant-name length. */
  .recurring-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto auto;
    align-items: center;
    gap: 0.75rem;
    padding: 0.55rem 0;
    border-top: 1px solid var(--color-text);
    border-top-color: color-mix(in srgb, var(--color-text) 12%, transparent);
  }

  .recurring-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.9rem;
  }

  .cadence-badge {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 0.15rem 0.45rem;
    border-radius: 999px;
    background-color: color-mix(in srgb, var(--color-text) 12%, transparent);
    opacity: 0.85;
  }

  .recurring-amount {
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    font-size: 0.95rem;
  }

  .recurring-meta {
    font-size: 0.75rem;
    opacity: 0.65;
    text-align: right;
    min-width: 9rem;
  }

  .lapsed-row {
    opacity: 0.55;
  }

  .lapsed {
    margin-top: 1.25rem;
  }

  .lapsed h3 {
    font-size: 0.85rem;
    margin: 0 0 0.2rem;
  }

  .lapsed .hint {
    display: block;
    margin-bottom: 0.4rem;
  }

  .link-button {
    margin-top: 0.6rem;
    padding: 0;
    border: none;
    background: none;
    color: var(--color-accent);
    font-size: 0.8rem;
    font-family: inherit;
    cursor: pointer;
  }

  .link-button:hover {
    text-decoration: underline;
  }

  .stats-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
    gap: 1rem;
    margin-top: 1.5rem;
  }

  .stats-row .month-card {
    margin-top: 0;
  }

  .charts-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 1rem;
    margin-top: 1.5rem;
  }

  .charts-row .chart-card {
    margin-top: 0;
  }

  .month-card h2 {
    font-size: 1rem;
    margin: 0 0 0.9rem;
  }

  .month-stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
    gap: 1rem;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .stat-amount {
    font-size: 1.25rem;
    font-weight: 700;
  }

  .stat-amount.muted {
    opacity: 0.5;
  }

  /* Spending more than last month is the unwelcome direction, so it takes the
     same color expenses have in the chart. */
  .stat-amount.over {
    color: var(--color-danger);
  }

  .stat-amount.under {
    color: var(--color-accent);
  }

  .hint {
    font-size: 0.75rem;
    opacity: 0.65;
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

  .chart-note {
    font-size: 0.8rem;
    opacity: 0.85;
  }

  .chart-note.up {
    color: var(--color-accent);
  }

  .chart-note.down {
    color: var(--color-danger);
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

  .balance-line {
    fill: none;
    stroke: var(--color-accent);
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  .balance-area {
    fill: var(--color-accent);
    fill-opacity: 0.12;
    stroke: none;
  }

  .column-hit {
    fill: transparent;
    pointer-events: all;
    cursor: default;
  }

  .balance-dot {
    fill: var(--color-accent);
    pointer-events: none;
    transform-box: fill-box;
    transform-origin: center;
    transition: transform 0.15s ease;
  }

  .column-hit:hover + .balance-dot {
    transform: scale(1.6);
  }

  .month-label {
    fill: var(--color-text);
    opacity: 0.7;
    font-size: 9px;
  }

  .axis-gridline {
    stroke: var(--color-text);
    stroke-opacity: 0.12;
    stroke-width: 1;
  }

  /* The zero baseline carries more meaning than the other gridlines now that
     the savings line can cross it. */
  .axis-gridline.zero {
    stroke-opacity: 0.35;
  }

  .axis-label {
    fill: var(--color-text);
    opacity: 0.6;
    font-size: 8px;
    dominant-baseline: middle;
  }
</style>
