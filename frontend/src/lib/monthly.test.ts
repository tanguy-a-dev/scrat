import { describe, expect, it } from "vitest";

import type { TransactionDto } from "$lib/api";
import {
  buildMonthlyTotals,
  buildScale,
  formatShortDate,
  isoDate,
  meanOf,
  monthKey,
  monthKeyOffsetFrom,
  niceStep,
  scaleY,
  spendDeltaPercent,
  spentInMonth,
  spentUpToDayOfMonth,
} from "./monthly";

let nextId = 0;

function txn(date: string, amountMinorUnits: number, categoryId = "food"): TransactionDto {
  nextId += 1;
  return {
    id: `t${nextId}`,
    date,
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

/* Local-time constructor, matching how the page captures "now". */
const MARCH_14 = new Date(2026, 2, 14);

describe("isoDate", () => {
  it("formats a local date with zero-padded parts", () => {
    expect(isoDate(new Date(2026, 0, 5))).toBe("2026-01-05");
    expect(isoDate(new Date(2026, 11, 31))).toBe("2026-12-31");
  });

  /* `toISOString()` converts to UTC first, so a local midnight east of
     Greenwich comes back as the previous day. This builds the string from the
     local calendar fields instead, which is why a local midnight survives. */
  it("keeps a local midnight on its own day", () => {
    expect(isoDate(new Date(2026, 2, 14, 0, 0, 0))).toBe("2026-03-14");
    expect(isoDate(new Date(2026, 2, 14, 23, 59, 59))).toBe("2026-03-14");
  });
});

describe("monthKey", () => {
  it("is the YYYY-MM prefix a transaction date slices to", () => {
    expect(monthKey(new Date(2026, 2, 14))).toBe("2026-03");
    expect(isoDate(new Date(2026, 2, 14)).slice(0, 7)).toBe(monthKey(new Date(2026, 2, 14)));
  });

  it("steps back across a year boundary", () => {
    expect(monthKeyOffsetFrom(new Date(2026, 0, 15), -1)).toBe("2025-12");
    expect(monthKeyOffsetFrom(new Date(2026, 0, 15), -13)).toBe("2024-12");
  });
});

describe("formatShortDate", () => {
  it("renders a day and abbreviated month", () => {
    expect(formatShortDate("2026-08-12")).toBe("12 Aug");
  });

  it("drops the leading zero from the day", () => {
    expect(formatShortDate("2026-08-05")).toBe("5 Aug");
  });

  /* `new Date("2026-08-12")` is UTC midnight, which is 11 August anywhere west
     of Greenwich. Splitting the string sidesteps the conversion entirely, so
     the rendered day matches the stored one in every timezone. */
  it("renders the stored day regardless of the host timezone", () => {
    expect(formatShortDate("2026-01-01")).toBe("1 Jan");
    expect(formatShortDate("2026-12-31")).toBe("31 Dec");
  });

  it("falls back to the raw string when the month is out of range", () => {
    expect(formatShortDate("2026-13-01")).toBe("2026-13-01");
    expect(formatShortDate("nonsense")).toBe("nonsense");
  });
});

describe("buildMonthlyTotals", () => {
  const noRent = new Set<string>();

  it("returns the requested number of months, oldest first, ending with today's", () => {
    const months = buildMonthlyTotals([], MARCH_14, 6, noRent);

    expect(months.map((m) => m.key)).toEqual([
      "2025-10",
      "2025-11",
      "2025-12",
      "2026-01",
      "2026-02",
      "2026-03",
    ]);
    expect(months.map((m) => m.label)).toEqual(["Oct", "Nov", "Dec", "Jan", "Feb", "Mar"]);
  });

  /* A month with nothing in it is a real, informative zero — leaving it out
     would silently compress the axis and misalign every bar. */
  it("zero-fills a month with no transactions", () => {
    const months = buildMonthlyTotals([txn("2026-03-02", -1_000)], MARCH_14, 3, noRent);

    expect(months.map((m) => m.expense)).toEqual([0, 0, 1_000]);
  });

  it("splits income from expenses and reports both as magnitudes", () => {
    const months = buildMonthlyTotals(
      [txn("2026-03-02", -1_000), txn("2026-03-05", 250_000)],
      MARCH_14,
      1,
      noRent,
    );

    expect(months[0].expense).toBe(1_000);
    expect(months[0].income).toBe(250_000);
  });

  it("derives savings as income minus expenses", () => {
    const months = buildMonthlyTotals(
      [txn("2026-03-02", -40_000), txn("2026-03-05", 250_000)],
      MARCH_14,
      1,
      noRent,
    );

    expect(months[0].savings).toBe(210_000);
  });

  /* An overspent month has to be able to go negative — clamping it at zero
     would make it look identical to breaking exactly even. */
  it("lets savings go negative in an overspent month", () => {
    const months = buildMonthlyTotals(
      [txn("2026-03-02", -300_000), txn("2026-03-05", 250_000)],
      MARCH_14,
      1,
      noRent,
    );

    expect(months[0].savings).toBe(-50_000);
  });

  /* The window is a view onto a wider fetch — the page deliberately asks for
     more months than any one chart plots, and open-endedly into the future. */
  it("ignores transactions outside the window", () => {
    const months = buildMonthlyTotals(
      [txn("2024-01-01", -99_999), txn("2030-01-01", -99_999), txn("2026-03-02", -1_000)],
      MARCH_14,
      3,
      noRent,
    );

    expect(months.reduce((sum, m) => sum + m.expense, 0)).toBe(1_000);
  });

  describe("rent exclusion", () => {
    const rent = new Set(["rent"]);

    it("keeps rent in the headline expense figure", () => {
      const months = buildMonthlyTotals(
        [txn("2026-03-02", -100_000, "rent"), txn("2026-03-05", -20_000, "food")],
        MARCH_14,
        1,
        rent,
      );

      expect(months[0].expense).toBe(120_000);
    });

    it("leaves rent out of the rent-free figure", () => {
      const months = buildMonthlyTotals(
        [txn("2026-03-02", -100_000, "rent"), txn("2026-03-05", -20_000, "food")],
        MARCH_14,
        1,
        rent,
      );

      expect(months[0].expenseWithoutRent).toBe(20_000);
    });

    it("makes the two figures identical when no rent category is configured", () => {
      const months = buildMonthlyTotals(
        [txn("2026-03-02", -100_000, "rent")],
        MARCH_14,
        1,
        new Set(),
      );

      expect(months[0].expenseWithoutRent).toBe(months[0].expense);
    });

    /* Rent is an expense, so excluding it must not touch income — an income
       row filed under the rent category is still income. */
    it("does not affect income", () => {
      const months = buildMonthlyTotals([txn("2026-03-02", 5_000, "rent")], MARCH_14, 1, rent);

      expect(months[0].income).toBe(5_000);
      expect(months[0].expenseWithoutRent).toBe(0);
    });
  });
});

describe("meanOf", () => {
  const months = buildMonthlyTotals(
    [txn("2026-02-02", -30_000), txn("2026-03-02", -10_000)],
    MARCH_14,
    2,
    new Set(),
  );

  /* Divided by the window length, not by the number of months that had
     activity — a quiet month is a real zero that should pull the mean down. */
  it("averages over the whole window including empty months", () => {
    expect(meanOf(months, "expense")).toBe(20_000);
  });

  it("rounds to whole minor units", () => {
    const uneven = buildMonthlyTotals(
      [txn("2026-02-02", -10_000), txn("2026-03-02", -1)],
      MARCH_14,
      3,
      new Set(),
    );

    expect(Number.isInteger(meanOf(uneven, "expense"))).toBe(true);
  });

  it("is zero rather than NaN for an empty window", () => {
    expect(meanOf([], "expense")).toBe(0);
  });
});

describe("spentInMonth", () => {
  it("sums only that month's expenses, as a magnitude", () => {
    const txns = [txn("2026-03-02", -1_000), txn("2026-02-28", -9_999), txn("2026-03-05", 50_000)];

    expect(spentInMonth(txns, "2026-03")).toBe(1_000);
  });

  it("is zero for a month with nothing in it", () => {
    expect(spentInMonth([txn("2026-03-02", -1_000)], "2026-01")).toBe(0);
  });
});

describe("spentUpToDayOfMonth", () => {
  const february = [
    txn("2026-02-01", -1_000),
    txn("2026-02-14", -2_000),
    txn("2026-02-28", -4_000),
  ];

  /* Comparing a part-month against a whole one would report a spending drop
     every single month until the last day of it. */
  it("counts only up to the given day", () => {
    expect(spentUpToDayOfMonth(february, "2026-02", 14)).toBe(3_000);
  });

  it("counts the whole month once the day covers it", () => {
    expect(spentUpToDayOfMonth(february, "2026-02", 28)).toBe(7_000);
  });

  /* On 31 March there is no 29-31 February to count, so the comparison is
     against all of February — which is all there was. */
  it("counts a whole shorter month when the day runs past its end", () => {
    expect(spentUpToDayOfMonth(february, "2026-02", 31)).toBe(7_000);
  });

  it("ignores income and other months", () => {
    const txns = [...february, txn("2026-02-02", 500_000), txn("2026-01-02", -8_000)];

    expect(spentUpToDayOfMonth(txns, "2026-02", 14)).toBe(3_000);
  });

  it("is zero on the first of the month before anything is spent", () => {
    expect(spentUpToDayOfMonth([txn("2026-02-14", -2_000)], "2026-02", 1)).toBe(0);
  });
});

describe("spendDeltaPercent", () => {
  it("reports the percentage change against last month to date", () => {
    expect(spendDeltaPercent(150, 100)).toBe(50);
    expect(spendDeltaPercent(50, 100)).toBe(-50);
    expect(spendDeltaPercent(100, 100)).toBe(0);
  });

  /* "Up 100% from nothing" is a division by zero, not a fact about spending —
     the strip omits the figure instead. */
  it("is null when there is nothing to compare against", () => {
    expect(spendDeltaPercent(5_000, 0)).toBeNull();
  });

  it("rounds to a whole percent", () => {
    expect(spendDeltaPercent(1_015, 1_000)).toBe(2);
  });
});

describe("niceStep", () => {
  it("rounds up to a 1/2/5 × 10^n value", () => {
    expect(niceStep(1)).toBe(1);
    expect(niceStep(1.5)).toBe(2);
    expect(niceStep(3)).toBe(5);
    expect(niceStep(7)).toBe(10);
    expect(niceStep(1_200)).toBe(2_000);
    expect(niceStep(45_000)).toBe(50_000);
  });

  it("never returns a step of zero", () => {
    expect(niceStep(0)).toBe(1);
    expect(niceStep(-5)).toBe(1);
    expect(niceStep(0.0001)).toBeGreaterThan(0);
  });
});

describe("buildScale", () => {
  it("covers the data with round tick values", () => {
    const scale = buildScale([0, 1_200, 4_800], true, 4);

    expect(scale.min).toBeLessThanOrEqual(0);
    expect(scale.max).toBeGreaterThanOrEqual(4_800);
    expect(scale.ticks[0]).toBe(scale.min);
    expect(scale.ticks[scale.ticks.length - 1]).toBe(scale.max);
  });

  it("spaces its ticks evenly", () => {
    const { ticks } = buildScale([0, 9_000], true, 4);
    const gaps = ticks.slice(1).map((t, i) => t - ticks[i]);

    expect(new Set(gaps).size).toBe(1);
  });

  /* Bars grow from the baseline, so the baseline has to be inside the domain
     even when every value sits well above it. */
  it("pulls zero into the domain when asked", () => {
    const scale = buildScale([5_000, 9_000], true, 4);

    expect(scale.min).toBeLessThanOrEqual(0);
  });

  it("leaves zero out when not asked", () => {
    const scale = buildScale([5_000, 9_000], false, 4);

    expect(scale.min).toBeGreaterThan(0);
  });

  /* An overspent month draws below the baseline; the domain has to reach
     there or the line is clipped off the chart. */
  it("reaches below zero for a negative series", () => {
    const scale = buildScale([-3_000, 5_000], true, 4);

    expect(scale.min).toBeLessThanOrEqual(-3_000);
    expect(scale.max).toBeGreaterThanOrEqual(5_000);
  });

  it("returns a usable unit domain for no data at all", () => {
    expect(buildScale([], true, 4)).toEqual({ min: 0, max: 1, ticks: [0, 1] });
  });

  /* Amounts are integer minor units, so a sub-unit step would produce ticks
     that round to the same label twice. */
  it("never steps by less than one minor unit", () => {
    const { ticks } = buildScale([0, 1], true, 4);

    expect(ticks[1] - ticks[0]).toBeGreaterThanOrEqual(1);
  });

  /* A perfectly flat series has no extent to spread ticks across, so the
     domain collapses onto the single value. That's degenerate but safe:
     `scaleY` guards the zero span, so the series draws on the baseline rather
     than producing NaN coordinates. The bar chart never reaches this case —
     it always passes `includeZero`, which gives the domain a floor. */
  it("collapses onto the value when every value is identical", () => {
    const scale = buildScale([5_000, 5_000], false, 4);

    expect(scale).toEqual({ min: 5_000, max: 5_000, ticks: [5_000] });
    expect(Number.isFinite(scaleY(5_000, scale, 194, 182))).toBe(true);
  });

  it("still spans a real domain for a flat series once zero is included", () => {
    const scale = buildScale([5_000, 5_000], true, 4);

    expect(scale.min).toBe(0);
    expect(scale.max).toBeGreaterThanOrEqual(5_000);
    expect(scale.ticks.length).toBeGreaterThan(1);
  });
});

describe("scaleY", () => {
  const scale = { min: 0, max: 100, ticks: [0, 50, 100] };
  const PLOT_BOTTOM = 194;
  const PLOT_HEIGHT = 182;

  /* SVG y grows downward, so the domain minimum sits at the bottom and the
     maximum at the top — inverting this silently flips every chart. */
  it("puts the domain minimum at the baseline and the maximum at the top", () => {
    expect(scaleY(0, scale, PLOT_BOTTOM, PLOT_HEIGHT)).toBe(PLOT_BOTTOM);
    expect(scaleY(100, scale, PLOT_BOTTOM, PLOT_HEIGHT)).toBe(PLOT_BOTTOM - PLOT_HEIGHT);
  });

  it("places the midpoint halfway up the plot", () => {
    expect(scaleY(50, scale, PLOT_BOTTOM, PLOT_HEIGHT)).toBeCloseTo(PLOT_BOTTOM - PLOT_HEIGHT / 2);
  });

  it("does not divide by zero on a collapsed domain", () => {
    const flat = { min: 5, max: 5, ticks: [5] };

    expect(Number.isFinite(scaleY(5, flat, PLOT_BOTTOM, PLOT_HEIGHT))).toBe(true);
  });
});
