/* The Overview page's arithmetic: bucketing transactions into calendar
   months, the month-to-date comparison, and the "nice" axis scale the charts
   share.

   Extracted from the page component so it can be tested directly. Everything
   here takes its "now" as a parameter rather than reading the clock, so a
   test can pin a date and the page can anchor every figure on the page to one
   captured instant. */

import type { TransactionDto } from "$lib/api";
import { shortMonthNames } from "$lib/i18n.svelte";

/** Chart-axis month labels in the interface language. A function rather than
 * a constant because the language can change while the app is running, and a
 * module-level array would be frozen at whatever it was on first import. */
export function monthLabels(): string[] {
  return shortMonthNames();
}

export interface MonthTotals {
  /** `YYYY-MM`, matching the prefix of a transaction's ISO date. */
  key: string;
  label: string;
  income: number;
  expense: number;
  /** `expense` minus anything filed under the configured rent category, so
   * the mean-spend card can show both figures. */
  expenseWithoutRent: number;
  savings: number;
}

export interface Scale {
  min: number;
  max: number;
  ticks: number[];
}

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

/** Local-calendar `YYYY-MM-DD`. Deliberately not `toISOString()`, which
 * converts to UTC first and so rolls a local midnight back to the previous day
 * anywhere east of Greenwich. */
export function isoDate(d: Date): string {
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

export function monthKey(d: Date): string {
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}`;
}

/** The month key `offset` months before `today` (negative offsets go back). */
export function monthKeyOffsetFrom(today: Date, offset: number): string {
  return monthKey(new Date(today.getFullYear(), today.getMonth() + offset, 1));
}

/** "12 Aug". Parsed by splitting the ISO string rather than via `new Date`,
 * which reads a bare `YYYY-MM-DD` as UTC midnight and so renders the previous
 * day for anyone west of Greenwich. */
export function formatShortDate(iso: string): string {
  const [, month, day] = iso.split("-");
  const label = monthLabels()[Number(month) - 1];
  return label ? `${Number(day)} ${label}` : iso;
}

/** The last `monthsCount` months (oldest first), zero-filled so a month with
 * no transactions still shows up as an empty bar rather than a gap.
 *
 * Transactions outside the window are ignored rather than clamped into the
 * nearest bucket — the fetch deliberately reaches further back (and is
 * open-ended forward) than any one chart's window. */
export function buildMonthlyTotals(
  txns: TransactionDto[],
  today: Date,
  monthsCount: number,
  rentCategoryIds: ReadonlySet<string>,
): MonthTotals[] {
  const months: MonthTotals[] = [];
  for (let i = monthsCount - 1; i >= 0; i--) {
    const d = new Date(today.getFullYear(), today.getMonth() - i, 1);
    months.push({
      key: monthKey(d),
      label: monthLabels()[d.getMonth()],
      income: 0,
      expense: 0,
      expenseWithoutRent: 0,
      savings: 0,
    });
  }
  const byKey = new Map(months.map((m) => [m.key, m]));
  for (const t of txns) {
    const bucket = byKey.get(t.date.slice(0, 7));
    if (!bucket) continue;
    if (t.amount_minor_units < 0) {
      bucket.expense += -t.amount_minor_units;
      if (!rentCategoryIds.has(t.category_id)) bucket.expenseWithoutRent += -t.amount_minor_units;
    } else {
      bucket.income += t.amount_minor_units;
    }
  }
  for (const m of months) m.savings = m.income - m.expense;
  return months;
}

/** Mean of one field across the window, rounded to whole minor units. The
 * window includes the current, partial month — the same months the bar chart
 * plots, so the card and the chart can't disagree. */
export function meanOf(months: MonthTotals[], field: keyof Omit<MonthTotals, "key" | "label">) {
  if (months.length === 0) return 0;
  return Math.round(months.reduce((sum, m) => sum + m[field], 0) / months.length);
}

/** Total spent in one calendar month, as a positive magnitude. */
export function spentInMonth(txns: TransactionDto[], key: string): number {
  return txns.reduce(
    (sum, t) => (t.amount_minor_units < 0 && t.date.slice(0, 7) === key ? sum - t.amount_minor_units : sum),
    0,
  );
}

/** Spending in `key`, counted only up to `dayOfMonth`, so a comparison made on
 * the 3rd isn't measured against a whole month.
 *
 * Days past the end of a shorter month simply contribute nothing, which is
 * what makes 31 March vs February work: there is no 29–31 February to count,
 * so the comparison is against all of February, which is all there was. */
export function spentUpToDayOfMonth(
  txns: TransactionDto[],
  key: string,
  dayOfMonth: number,
): number {
  return txns.reduce((sum, t) => {
    if (t.amount_minor_units >= 0) return sum;
    if (t.date.slice(0, 7) !== key) return sum;
    if (Number(t.date.slice(8, 10)) > dayOfMonth) return sum;
    return sum - t.amount_minor_units;
  }, 0);
}

/** Percent change against last month to date, or null when there's nothing to
 * compare against — "up 100% from zero" is not a fact about spending. */
export function spendDeltaPercent(thisMonth: number, lastMonthToDate: number): number | null {
  if (lastMonthToDate <= 0) return null;
  return Math.round(((thisMonth - lastMonthToDate) / lastMonthToDate) * 100);
}

/** Rounds a raw axis step up to a "nice" 1/2/5 × 10^n value, so the scale
 * reads as round numbers (e.g. 500, 1000) rather than awkward fractions. */
export function niceStep(rawStep: number): number {
  if (rawStep <= 0) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(rawStep));
  const residual = rawStep / magnitude;
  const niceResidual = residual <= 1 ? 1 : residual <= 2 ? 2 : residual <= 5 ? 5 : 10;
  return niceResidual * magnitude;
}

/** A nice-rounded [min, max] domain covering `values`, in minor units.
 * `includeZero` forces the baseline into the domain — needed by the bar chart
 * (bars grow from zero) and by any series that crosses into negative. */
export function buildScale(values: number[], includeZero: boolean, tickCount: number): Scale {
  if (values.length === 0) return { min: 0, max: 1, ticks: [0, 1] };
  const rawMax = Math.max(...values, ...(includeZero ? [0] : []));
  const rawMin = Math.min(...values, ...(includeZero ? [0] : []));
  // A step below one minor unit is meaningless — amounts are integers.
  const step = Math.max(1, niceStep(Math.max(rawMax - rawMin, 1) / tickCount));
  const min = Math.floor(rawMin / step) * step;
  const max = Math.ceil(rawMax / step) * step;
  const ticks: number[] = [];
  for (let v = min; v <= max + step * 0.001; v += step) ticks.push(Math.round(v));
  return { min, max, ticks };
}

/** Maps a value in the scale's domain to a y coordinate, with `plotBottom` the
 * baseline and `plotHeight` the drawable height above it. */
export function scaleY(
  value: number,
  scaleDef: Scale,
  plotBottom: number,
  plotHeight: number,
): number {
  const span = scaleDef.max - scaleDef.min || 1;
  return plotBottom - ((value - scaleDef.min) / span) * plotHeight;
}
