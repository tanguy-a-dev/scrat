import { describe, expect, it } from "vitest";

import type { CategoryDto, TransactionDto } from "$lib/api";
import {
  buildBreakdown,
  buildPanelRows,
  buildRootMap,
  compareArcs,
  compareRingWidth,
  gapped,
  hiddenRows,
  withAnimatedSlices,
  withDonutSlices,
  type DonutGeometry,
} from "./breakdown";

const RADIUS = 80;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;
const PALETTE = ["c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7"];
const GEOMETRY: DonutGeometry = {
  circumference: CIRCUMFERENCE,
  sliceGap: 2,
  palette: PALETTE,
};

let nextId = 0;

function txn(categoryId: string, amountMinorUnits: number): TransactionDto {
  nextId += 1;
  return {
    id: `t${nextId}`,
    date: "2026-03-14",
    amount_minor_units: amountMinorUnits,
    currency: "EUR",
    description: "whatever",
    category_id: categoryId,
    account_id: "a1",
    role: "normal",
    transfer_group_id: null,
    operation_kind: "card",
  };
}

function cat(id: string, name: string, parentId: string | null = null): CategoryDto {
  return { id, name, parent_id: parentId, icon: null, is_default: false };
}

/* Housing > Rent, Housing > Utilities, plus a flat Food root. */
const CATEGORIES = [
  cat("housing", "Housing"),
  cat("rent", "Rent", "housing"),
  cat("utilities", "Utilities", "housing"),
  cat("food", "Food"),
];

const ROOTS = buildRootMap(CATEGORIES);
const rootOf = (id: string) => ROOTS.get(id) ?? id;
const nameOf = (id: string) => CATEGORIES.find((c) => c.id === id)?.name ?? "Uncategorized";

describe("buildRootMap", () => {
  it("maps a root category to itself", () => {
    expect(rootOf("housing")).toBe("housing");
    expect(rootOf("food")).toBe("food");
  });

  it("maps a subcategory to its parent", () => {
    expect(rootOf("rent")).toBe("housing");
    expect(rootOf("utilities")).toBe("housing");
  });

  /* A transaction can reference a category the list doesn't carry (e.g. one
     deleted between two loads). It has to resolve to something rather than
     `undefined`, or it would silently vanish from every total. */
  it("leaves an unknown category as its own root", () => {
    expect(rootOf("never-seen")).toBe("never-seen");
  });

  /* Not reachable through the domain rules, which forbid it — but a corrupt
     row must not spin the page into an infinite walk. */
  it("terminates on a category that points at itself", () => {
    const roots = buildRootMap([cat("loop", "Loop", "loop")]);
    expect(roots.get("loop")).toBe("loop");
  });
});

describe("buildBreakdown", () => {
  it("rolls subcategories up into their root", () => {
    const { total, breakdown } = buildBreakdown(
      [txn("rent", -100_000), txn("utilities", -20_000), txn("food", -30_000)],
      null,
      rootOf,
      nameOf,
    );

    expect(total).toBe(150_000);
    expect(breakdown.map((r) => [r.categoryId, r.amountMinorUnits])).toEqual([
      ["housing", 120_000],
      ["food", 30_000],
    ]);
  });

  /* Expenses arrive as negative minor units and are reported as magnitudes —
     a donut of negative arcs would paint nothing. */
  it("sums magnitudes regardless of sign", () => {
    const { total } = buildBreakdown([txn("food", -1_000), txn("food", 400)], null, rootOf, nameOf);

    expect(total).toBe(1_400);
  });

  it("gives each category its share of the total", () => {
    const { breakdown } = buildBreakdown(
      [txn("housing", -75_000), txn("food", -25_000)],
      null,
      rootOf,
      nameOf,
    );

    expect(breakdown[0].percent).toBeCloseTo(75);
    expect(breakdown[1].percent).toBeCloseTo(25);
  });

  it("sorts largest first", () => {
    const { breakdown } = buildBreakdown(
      [txn("food", -1_000), txn("housing", -9_000), txn("rent", -5_000)],
      null,
      rootOf,
      nameOf,
    );

    expect(breakdown.map((r) => r.categoryId)).toEqual(["housing", "food"]);
    expect(breakdown[0].amountMinorUnits).toBe(14_000);
  });

  /* Division by a zero total would put a literal "NaN%" on screen and make
     every arc unpaintable. */
  it("reports zero shares rather than NaN for an empty panel", () => {
    const { total, breakdown } = buildBreakdown([], null, rootOf, nameOf);

    expect(total).toBe(0);
    expect(breakdown).toEqual([]);
  });

  it("resolves each row's display name", () => {
    const { breakdown } = buildBreakdown([txn("rent", -100)], null, rootOf, nameOf);

    expect(breakdown[0].name).toBe("Housing");
  });

  describe("drilldown", () => {
    it("keeps only the scoped root's transactions and groups by subcategory", () => {
      const { total, breakdown } = buildBreakdown(
        [txn("rent", -100_000), txn("utilities", -20_000), txn("food", -30_000)],
        "housing",
        rootOf,
        nameOf,
      );

      expect(total).toBe(120_000);
      expect(breakdown.map((r) => [r.categoryId, r.amountMinorUnits])).toEqual([
        ["rent", 100_000],
        ["utilities", 20_000],
      ]);
    });

    /* Shares inside a drilldown are of the parent's total, not the panel's —
       Rent is 83% of Housing even when Housing is half the month's spend. */
    it("computes shares against the scoped total", () => {
      const { breakdown } = buildBreakdown(
        [txn("rent", -100_000), txn("utilities", -20_000), txn("food", -1_000_000)],
        "housing",
        rootOf,
        nameOf,
      );

      expect(breakdown[0].percent).toBeCloseTo((100 / 120) * 100);
    });

    /* Money filed directly against the parent is its own row alongside the
       children, rather than being dropped for having no subcategory. */
    it("keeps amounts booked straight to the root", () => {
      const { breakdown } = buildBreakdown(
        [txn("housing", -5_000), txn("rent", -10_000)],
        "housing",
        rootOf,
        nameOf,
      );

      expect(breakdown.map((r) => r.categoryId)).toEqual(["rent", "housing"]);
    });
  });
});

describe("hiddenRows", () => {
  it("reports only the hidden categories, with their real amounts", () => {
    const rows = hiddenRows(
      [txn("housing", -10_000), txn("food", -3_000)],
      new Set(["food"]),
      null,
      rootOf,
      nameOf,
    );

    expect(rows).toEqual([{ categoryId: "food", name: "Food", amountMinorUnits: 3_000 }]);
  });

  it("is empty when nothing is hidden", () => {
    expect(hiddenRows([txn("food", -3_000)], new Set(), null, rootOf, nameOf)).toEqual([]);
  });

  it("rolls a hidden root's subcategories into it", () => {
    const rows = hiddenRows(
      [txn("rent", -10_000), txn("utilities", -2_000)],
      new Set(["housing"]),
      null,
      rootOf,
      nameOf,
    );

    expect(rows).toEqual([{ categoryId: "housing", name: "Housing", amountMinorUnits: 12_000 }]);
  });

  it("sorts largest first", () => {
    const rows = hiddenRows(
      [txn("food", -1_000), txn("housing", -9_000)],
      new Set(["food", "housing"]),
      null,
      rootOf,
      nameOf,
    );

    expect(rows.map((r) => r.categoryId)).toEqual(["housing", "food"]);
  });
});

describe("buildPanelRows", () => {
  const build = (
    a: TransactionDto[],
    b: TransactionDto[],
    compareActive: boolean,
    scopeRootId: string | null = null,
  ) => buildPanelRows(a, b, scopeRootId, compareActive, rootOf, nameOf, GEOMETRY);

  it("zeroes the comparison period when comparison is off", () => {
    const { totalB, rows } = build([txn("food", -1_000)], [txn("food", -9_999)], false);

    expect(totalB).toBe(0);
    expect(rows[0].amountB).toBe(0);
    expect(rows[0].percentB).toBe(0);
  });

  it("pairs a category's two periods", () => {
    const { rows } = build([txn("food", -1_000)], [txn("food", -800)], true);

    expect(rows[0].amountMinorUnits).toBe(1_000);
    expect(rows[0].amountB).toBe(800);
    expect(rows[0].deltaMinor).toBe(200);
    expect(rows[0].deltaRatio).toBeCloseTo(0.25);
  });

  /* The single most important property of the comparison: a category that
     disappeared is the biggest thing that changed, and reporting only period
     A's categories would omit it entirely. */
  it("includes a category present only in the comparison period", () => {
    const { rows } = build([txn("food", -1_000)], [txn("housing", -50_000)], true);

    const housing = rows.find((r) => r.categoryId === "housing");
    expect(housing).toBeDefined();
    expect(housing!.amountMinorUnits).toBe(0);
    expect(housing!.amountB).toBe(50_000);
    expect(housing!.deltaMinor).toBe(-50_000);
  });

  /* "Up ∞%" is a division by zero wearing a percentage sign. The row has to
     say "new" instead, which it can only do if this is null. */
  it("reports a brand-new category's ratio as null rather than Infinity", () => {
    const { rows } = build([txn("food", -1_000)], [], true);

    expect(rows[0].amountB).toBe(0);
    expect(rows[0].deltaRatio).toBeNull();
    expect(rows[0].deltaRatio).not.toBe(Infinity);
  });

  it("reports a vanished category's ratio as -1", () => {
    const { rows } = build([], [txn("food", -1_000)], true);

    expect(rows[0].deltaRatio).toBe(-1);
  });

  /* Sorting by period A alone would rank a category that went from €500 to
     nothing below one that went from €0 to €5 — burying the change the user
     most needs to see. */
  it("ranks by whichever period the category was larger in", () => {
    const { rows } = build(
      [txn("food", -500)],
      [txn("housing", -50_000), txn("food", -100)],
      true,
    );

    expect(rows.map((r) => r.categoryId)).toEqual(["housing", "food"]);
  });

  it("scopes to one root when drilling down", () => {
    const { rows } = build(
      [txn("rent", -10_000), txn("food", -99_000)],
      [txn("rent", -8_000)],
      true,
      "housing",
    );

    expect(rows.map((r) => r.categoryId)).toEqual(["rent"]);
    expect(rows[0].amountB).toBe(8_000);
  });

  it("attaches donut geometry to every row", () => {
    const { rows } = build([txn("food", -750), txn("housing", -250)], [], false);

    expect(rows[0].color).toBe(PALETTE[0]);
    expect(rows[1].color).toBe(PALETTE[1]);
    expect(rows[0].dashoffset).toBe(-0);
  });
});

describe("gapped", () => {
  /* A lone slice has nothing to be separated from — carving a spacer out of a
     ring it owns entirely would leave a visible notch for no reason. */
  it("leaves a lone slice at full length", () => {
    expect(gapped(100, 1, 2)).toBe(100);
    expect(gapped(100, 0, 2)).toBe(100);
  });

  it("shrinks a slice by the gap when it has neighbours", () => {
    expect(gapped(100, 3, 2)).toBe(98);
  });

  /* A sliver must stay visible rather than be eaten — or worse, given a
     negative length, which paints as a full ring. */
  it("never shrinks a slice past half its own length", () => {
    expect(gapped(3, 5, 2)).toBe(1.5);
    expect(gapped(0.5, 5, 2)).toBe(0.25);
    expect(gapped(1, 5, 2)).toBeGreaterThan(0);
  });
});

describe("withDonutSlices", () => {
  const slices = (percents: number[]) =>
    withDonutSlices(
      percents.map((percent) => ({ percent })),
      GEOMETRY,
    );

  /* Each slice starts where the previous one ended. Getting this wrong stacks
     every arc at twelve o'clock. */
  it("offsets each slice by the total length of those before it", () => {
    const [a, b, c] = slices([50, 25, 25]);

    expect(a.dashoffset).toBeCloseTo(-0);
    expect(b.dashoffset).toBeCloseTo(-CIRCUMFERENCE * 0.5);
    expect(c.dashoffset).toBeCloseTo(-CIRCUMFERENCE * 0.75);
  });

  it("consumes exactly one full ring when the shares sum to 100", () => {
    const result = slices([50, 30, 20]);
    const lastEnd = -result[2].dashoffset + (result[2].percent / 100) * CIRCUMFERENCE;

    expect(lastEnd).toBeCloseTo(CIRCUMFERENCE);
  });

  it("cycles the palette past its last colour", () => {
    const result = slices(new Array(10).fill(10));

    expect(result[8].color).toBe(PALETTE[0]);
    expect(result[9].color).toBe(PALETTE[1]);
  });

  /* Comparison-only rows carry percent 0 and draw nothing, so they must not
     make a genuinely solo slice think it has a neighbour to gap against. */
  it("ignores zero-share rows when deciding whether a gap is needed", () => {
    const [drawn] = slices([100, 0, 0]);

    expect(drawn.dasharray).toBe(`${CIRCUMFERENCE} 0`);
  });

  it("gaps a slice that does have a drawn neighbour", () => {
    const [first] = slices([60, 40]);
    const [length] = first.dasharray.split(" ").map(Number);

    expect(length).toBeCloseTo(CIRCUMFERENCE * 0.6 - GEOMETRY.sliceGap);
  });

  it("returns an empty ring for an empty breakdown", () => {
    expect(slices([])).toEqual([]);
  });
});

describe("withAnimatedSlices", () => {
  const base = withDonutSlices([{ percent: 60 }, { percent: 40 }], GEOMETRY);

  it("draws nothing at the start of the sweep", () => {
    const animated = withAnimatedSlices(base, GEOMETRY, 0);

    expect(animated[0].animatedLength).toBe(0);
    expect(animated[0].animatedDashoffset).toBe(-0);
    expect(animated[0].animatedPercent).toBe(0);
  });

  it("matches the static geometry at the end of the sweep", () => {
    const animated = withAnimatedSlices(base, GEOMETRY, 1);

    for (const [i, slice] of animated.entries()) {
      const [staticLength] = base[i].dasharray.split(" ").map(Number);
      expect(slice.animatedLength).toBeCloseTo(staticLength);
      expect(slice.animatedDashoffset).toBeCloseTo(base[i].dashoffset);
    }
  });

  /* Every slice is scaled by the same progress value, which is what makes the
     ring sweep as one shape instead of each arc racing independently. */
  it("scales every slice by the same fraction mid-sweep", () => {
    const animated = withAnimatedSlices(base, GEOMETRY, 0.5);

    expect(animated[0].animatedPercent).toBeCloseTo(30);
    expect(animated[1].animatedPercent).toBeCloseTo(20);
  });
});

describe("compareArcs", () => {
  const COMPARE_RADIUS = 54;
  const COMPARE_CIRCUMFERENCE = 2 * Math.PI * COMPARE_RADIUS;

  it("walks its own circumference, not the main ring's", () => {
    const [only] = compareArcs([{ percentB: 100 }], COMPARE_RADIUS, 2, 1);
    const [length] = only.arcDasharray.split(" ").map(Number);

    expect(length).toBeCloseTo(COMPARE_CIRCUMFERENCE);
    expect(length).not.toBeCloseTo(CIRCUMFERENCE);
  });

  /* The cumulative walk uses each row's full length even while the drawn
     length is scaled, so slices stay in their final positions as the ring
     sweeps in rather than sliding around. */
  it("offsets each arc by those before it", () => {
    const arcs = compareArcs([{ percentB: 50 }, { percentB: 50 }], COMPARE_RADIUS, 2, 1);

    expect(arcs[0].arcDashoffset).toBeCloseTo(-0);
    expect(arcs[1].arcDashoffset).toBeCloseTo(-COMPARE_CIRCUMFERENCE * 0.5);
  });

  it("draws nothing at the start of the sweep", () => {
    const [arc] = compareArcs([{ percentB: 100 }], COMPARE_RADIUS, 2, 0);

    expect(arc.arcDasharray).toBe(`0 ${COMPARE_CIRCUMFERENCE}`);
  });

  it("ignores zero-share rows when deciding whether a gap is needed", () => {
    const [drawn] = compareArcs([{ percentB: 100 }, { percentB: 0 }], COMPARE_RADIUS, 2, 1);
    const [length] = drawn.arcDasharray.split(" ").map(Number);

    expect(length).toBeCloseTo(COMPARE_CIRCUMFERENCE);
  });
});

describe("compareRingWidth", () => {
  const width = (a: number, b: number) => compareRingWidth(a, b, 11, 7, 15);

  it("matches the base width when both periods are equal", () => {
    expect(width(1_000, 1_000)).toBe(11);
  });

  it("thickens for a bigger comparison period and thins for a smaller one", () => {
    expect(width(1_000, 1_200)).toBeGreaterThan(11);
    expect(width(1_000, 800)).toBeLessThan(11);
  });

  /* Unclamped, a tenfold difference would draw a band thicker than the ring
     it nests inside, or a hairline too thin to see. */
  it("clamps at both ends however extreme the ratio", () => {
    expect(width(1_000, 1_000_000)).toBe(15);
    expect(width(1_000, 1)).toBe(7);
  });

  /* An empty period A makes the ratio a division by zero — the ring falls
     back to its base width rather than becoming NaN and disappearing. */
  it("falls back to the base width when the primary period is empty", () => {
    expect(width(0, 5_000)).toBe(11);
    expect(width(-1, 5_000)).toBe(11);
  });
});
