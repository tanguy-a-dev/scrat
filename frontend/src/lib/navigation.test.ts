import { describe, expect, it } from "vitest";

import { adjacentPageHref, navPages } from "./navigation";

describe("adjacentPageHref", () => {
  it("walks forward through the nav order", () => {
    expect(adjacentPageHref("/overview", 1)).toBe("/details");
    expect(adjacentPageHref("/details", 1)).toBe("/transactions");
  });

  it("walks backward through the nav order", () => {
    expect(adjacentPageHref("/details", -1)).toBe("/overview");
    expect(adjacentPageHref("/transactions", -1)).toBe("/details");
  });

  /* The shortcut cycles rather than dead-ending, so holding it never leaves
     the user stuck on the first or last page wondering if the key broke. */
  it("wraps around at both ends", () => {
    const first = navPages[0].href;
    const last = navPages[navPages.length - 1].href;

    expect(adjacentPageHref(last, 1)).toBe(first);
    expect(adjacentPageHref(first, -1)).toBe(last);
  });

  /* The unlock screen ("/") isn't in the nav list. Pressing the shortcut
     there has to land somewhere sensible rather than on `undefined.href`. */
  it("falls back to the first page from a pathname outside the nav", () => {
    expect(adjacentPageHref("/", 1)).toBe(navPages[1].href);
    expect(adjacentPageHref("/nonexistent", 1)).toBe(navPages[1].href);
  });

  it("visits every page exactly once before returning to the start", () => {
    const visited: string[] = [];
    let current = navPages[0].href;
    for (let i = 0; i < navPages.length; i++) {
      visited.push(current);
      current = adjacentPageHref(current, 1);
    }

    expect(visited).toEqual(navPages.map((p) => p.href));
    expect(current).toBe(navPages[0].href);
  });
});
