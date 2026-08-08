/* The Details page's arithmetic: rolling transactions up into per-category
   rows, pairing two periods against each other, and turning the resulting
   shares into donut arc geometry.

   Extracted from the page component so it can be tested directly. None of it
   touches Svelte state — every input a caller needs is a parameter, which is
   also what lets the same functions serve the main ring, the comparison ring
   and the nested subcategory rows without three copies. */

import type { CategoryDto, TransactionDto } from "$lib/api";

/** Resolves a category id to the id of the root it belongs under. A root
 * resolves to itself. */
export type RootResolver = (categoryId: string) => string;

/** Resolves a category id to its display name. */
export type NameResolver = (categoryId: string) => string;

export interface BreakdownRow {
  categoryId: string;
  name: string;
  amountMinorUnits: number;
  percent: number;
}

export interface Breakdown {
  total: number;
  breakdown: BreakdownRow[];
}

/** One category's line in a panel: always its period-A figures, plus its
 * period-B ones when comparing (zeros when not, so one code path serves both
 * views).
 *
 * `deltaRatio` is null rather than Infinity when the category had nothing in
 * period B. "Up ∞%" is not a fact about spending, it's a division by zero
 * wearing a percentage sign — the row says "new" instead. */
export interface PanelRow extends BreakdownRow {
  amountB: number;
  percentB: number;
  deltaMinor: number;
  deltaRatio: number | null;
}

export interface DonutSlice {
  color: string;
  dasharray: string;
  dashoffset: number;
}

export interface DonutGeometry {
  /** Arc length of the full ring, in viewBox units. */
  circumference: number;
  /** Surface left between neighbouring slices, in viewBox units. */
  sliceGap: number;
  palette: readonly string[];
}

/** Maps every category id to the id of the root it sits under, following
 * `parent_id` upward.
 *
 * Memoized as it walks: a flat category list can name a parent before the
 * parent's own row appears, so resolving each id independently would re-walk
 * the same chains repeatedly. The two-level hierarchy makes those chains
 * short, but the cache also makes the walk safe against a row that somehow
 * points at itself. */
export function buildRootMap(categories: CategoryDto[]): Map<string, string> {
  const byId = new Map(categories.map((c) => [c.id, c]));
  const cache = new Map<string, string>();

  function findRoot(id: string, seen: Set<string>): string {
    const cached = cache.get(id);
    if (cached !== undefined) return cached;
    /* A cycle can't arise from the domain rules, but a corrupt row must not
       hang the page — treat the id we looped back to as its own root. */
    if (seen.has(id)) return id;
    seen.add(id);
    const category = byId.get(id);
    const root = category?.parent_id ? findRoot(category.parent_id, seen) : id;
    cache.set(id, root);
    return root;
  }

  for (const c of categories) findRoot(c.id, new Set());
  return cache;
}

/** Sums transactions per category and turns each into a share of the total.
 *
 * `scopeRootId` narrows to one root category's transactions and groups by its
 * subcategories instead of by root — this is what powers drilldown. */
export function buildBreakdown(
  txns: TransactionDto[],
  scopeRootId: string | null,
  rootOf: RootResolver,
  nameOf: NameResolver,
): Breakdown {
  const scoped = scopeRootId ? txns.filter((t) => rootOf(t.category_id) === scopeRootId) : txns;
  const total = scoped.reduce((sum, t) => sum + Math.abs(t.amount_minor_units), 0);
  const sums = new Map<string, number>();
  for (const t of scoped) {
    const key = scopeRootId ? t.category_id : rootOf(t.category_id);
    sums.set(key, (sums.get(key) ?? 0) + Math.abs(t.amount_minor_units));
  }
  /* An empty panel divides by one rather than zero — every share is then a
     well-defined 0% instead of NaN, which would reach the DOM as a literal
     "NaN%" and an unpaintable arc. */
  const totalOrOne = total || 1;
  const breakdown = [...sums.entries()]
    .map(([categoryId, amountMinorUnits]) => ({
      categoryId,
      name: nameOf(categoryId),
      amountMinorUnits,
      percent: (amountMinorUnits / totalOrOne) * 100,
    }))
    .sort((a, b) => b.amountMinorUnits - a.amountMinorUnits);
  return { total, breakdown };
}

/** Rows for the categories a panel is currently hiding: they carry their real
 * amount (so the user can see what they're leaving out) but no share, since by
 * definition they're no longer part of the total the shares are of.
 *
 * At the top level (`scopeRootId` null), hiding a *root* rolls its
 * subcategories up into one "Housing"-style row, same as the main breakdown
 * rolls up a visible root. Hiding one subcategory while the root still has a
 * visible sibling is deliberately left off this list — the expanded drilldown
 * already shows that hidden subcategory with a working eye, and duplicating
 * it here would just be clutter. But once every one of a root's subcategories
 * is hidden this way, the root has nothing visible left to anchor a row in
 * the main breakdown either, and it would vanish from the panel with no eye
 * left anywhere to click — so that case surfaces each such subcategory here
 * under its own name instead of the root's, which also keeps the eye button
 * toggling the id that's actually in `hidden`. */
export function hiddenRows(
  txns: TransactionDto[],
  hidden: ReadonlySet<string>,
  scopeRootId: string | null,
  rootOf: RootResolver,
  nameOf: NameResolver,
): Omit<BreakdownRow, "percent">[] {
  let fullyHiddenRoots = new Set<string>();
  if (scopeRootId === null) {
    const rootsWithVisible = new Set<string>();
    const rootsWithHidden = new Set<string>();
    for (const t of txns) {
      const root = rootOf(t.category_id);
      const isHiddenTxn = hidden.has(t.category_id) || hidden.has(root);
      (isHiddenTxn ? rootsWithHidden : rootsWithVisible).add(root);
    }
    fullyHiddenRoots = new Set([...rootsWithHidden].filter((r) => !rootsWithVisible.has(r)));
  }

  const sums = new Map<string, number>();
  for (const t of txns) {
    const root = rootOf(t.category_id);
    if (scopeRootId !== null && root !== scopeRootId) continue;

    let key: string;
    if (scopeRootId !== null) {
      key = t.category_id;
    } else if (hidden.has(root)) {
      key = root;
    } else if (hidden.has(t.category_id) && fullyHiddenRoots.has(root)) {
      key = t.category_id;
    } else {
      continue;
    }
    if (!hidden.has(key)) continue;
    sums.set(key, (sums.get(key) ?? 0) + Math.abs(t.amount_minor_units));
  }
  return [...sums.entries()]
    .map(([categoryId, amountMinorUnits]) => ({
      categoryId,
      name: nameOf(categoryId),
      amountMinorUnits,
    }))
    .sort((a, b) => b.amountMinorUnits - a.amountMinorUnits);
}

/** Merges a panel's two periods into one list of rows.
 *
 * The union, not period A's categories: a category with €300 in June and
 * nothing in August has to appear, or the comparison quietly omits the single
 * biggest thing that changed. Those rows carry `amountMinorUnits: 0`, so they
 * take no donut slice — they exist in the list, which is where the comparison
 * actually lives.
 *
 * Sorting is by whichever period the category was larger in, rather than by
 * period A. Sorting by A alone would drop every disappeared category into a
 * silent block at the bottom, ranked below rows a hundredth their size. */
export function buildPanelRows(
  txnsA: TransactionDto[],
  txnsB: TransactionDto[],
  scopeRootId: string | null,
  compareActive: boolean,
  rootOf: RootResolver,
  nameOf: NameResolver,
  geometry: DonutGeometry,
): { total: number; totalB: number; rows: (PanelRow & DonutSlice)[] } {
  const a = buildBreakdown(txnsA, scopeRootId, rootOf, nameOf);
  const b = compareActive
    ? buildBreakdown(txnsB, scopeRootId, rootOf, nameOf)
    : { total: 0, breakdown: [] as BreakdownRow[] };
  const bById = new Map(b.breakdown.map((r) => [r.categoryId, r]));

  const row = (
    categoryId: string,
    name: string,
    amountMinorUnits: number,
    percent: number,
  ): PanelRow => {
    const match = bById.get(categoryId);
    const amountB = match?.amountMinorUnits ?? 0;
    return {
      categoryId,
      name,
      amountMinorUnits,
      percent,
      amountB,
      percentB: match?.percent ?? 0,
      deltaMinor: amountMinorUnits - amountB,
      deltaRatio: amountB === 0 ? null : (amountMinorUnits - amountB) / amountB,
    };
  };

  const rows = a.breakdown.map((r) => row(r.categoryId, r.name, r.amountMinorUnits, r.percent));
  if (compareActive) {
    const inA = new Set(a.breakdown.map((r) => r.categoryId));
    for (const r of b.breakdown) {
      if (!inA.has(r.categoryId)) rows.push(row(r.categoryId, r.name, 0, 0));
    }
  }
  rows.sort(
    (x, y) => Math.max(y.amountMinorUnits, y.amountB) - Math.max(x.amountMinorUnits, x.amountB),
  );
  return { total: a.total, totalB: b.total, rows: withDonutSlices(rows, geometry) };
}

/** Shrinks an arc by the gap, but never past half its own length — a very
 * small slice should stay visible rather than be eaten by the spacer. A lone
 * slice has nothing to be separated from, so it keeps the full ring. */
export function gapped(length: number, sliceCount: number, sliceGap: number): number {
  if (sliceCount < 2) return length;
  return Math.max(length - sliceGap, length / 2);
}

/** Walks the rows once, converting each share into a stroke-dasharray arc and
 * a stroke-dashoffset that places it after everything before it. */
export function withDonutSlices<T extends { percent: number }>(
  breakdown: T[],
  geometry: DonutGeometry,
): (T & DonutSlice)[] {
  const { circumference, sliceGap, palette } = geometry;
  let cumulative = 0;
  /* Rows the comparison added for categories absent from this period draw no
     arc, so they must not be counted when deciding whether there is a
     neighbour to leave a gap against — otherwise a panel showing one real
     category would carve a spacer out of a ring it has entirely to itself. */
  const drawnCount = breakdown.filter((s) => s.percent > 0).length;
  return breakdown.map((slice, i) => {
    const length = (slice.percent / 100) * circumference;
    const dashoffset = -cumulative;
    cumulative += length;
    const drawn = gapped(length, drawnCount, sliceGap);
    return {
      ...slice,
      color: palette[i % palette.length],
      dasharray: `${drawn} ${circumference - drawn}`,
      dashoffset,
    };
  });
}

/** Scales each slice's arc length/offset by `fillProgress`, so the whole donut
 * sweeps in from empty together rather than each slice animating
 * independently out of sync with the others. */
export function withAnimatedSlices<T extends { percent: number; dashoffset: number }>(
  slices: T[],
  geometry: DonutGeometry,
  fillProgress: number,
): (T & { animatedLength: number; animatedDashoffset: number; animatedPercent: number })[] {
  const drawnCount = slices.filter((s) => s.percent > 0).length;
  return slices.map((slice) => ({
    ...slice,
    animatedLength: gapped(
      (slice.percent / 100) * geometry.circumference * fillProgress,
      drawnCount,
      geometry.sliceGap,
    ),
    animatedDashoffset: slice.dashoffset * fillProgress,
    animatedPercent: slice.percent * fillProgress,
  }));
}

/** The comparison ring's arcs, built from `percentB` the same way
 * `withDonutSlices` + `withAnimatedSlices` build the main ring's from
 * `percent` — its own circumference, its own cumulative walk, sharing the row
 * order so both rings run through the categories in the same sequence and the
 * same colours. */
export function compareArcs<T extends { percentB: number }>(
  rows: T[],
  radius: number,
  sliceGap: number,
  fillProgress: number,
): (T & { arcDasharray: string; arcDashoffset: number })[] {
  const circumference = 2 * Math.PI * radius;
  const drawnCount = rows.filter((r) => r.percentB > 0).length;
  let cumulative = 0;
  return rows.map((row) => {
    const full = (row.percentB / 100) * circumference;
    const dashoffset = -cumulative * fillProgress;
    cumulative += full;
    const drawn = gapped(full * fillProgress, drawnCount, sliceGap);
    return {
      ...row,
      arcDasharray: `${drawn} ${circumference - drawn}`,
      arcDashoffset: dashoffset,
    };
  });
}

/** Thickness carries the period's total, so a month that cost more is visibly
 * fatter — the magnitude a share-based chart otherwise throws away entirely.
 *
 * Clamped hard, and deliberately narrow. Width is a weak perceptual channel,
 * the exact figures are already printed in the middle of the ring, and an
 * unclamped ratio would turn a quiet month into a hairline and a bad one into
 * a band thicker than the ring it is nested in. It is a nudge, not a
 * measurement. */
export function compareRingWidth(
  totalA: number,
  totalB: number,
  base: number,
  min: number,
  max: number,
): number {
  if (totalA <= 0) return base;
  const scaled = base * (totalB / totalA);
  return Math.min(Math.max(scaled, min), max);
}
