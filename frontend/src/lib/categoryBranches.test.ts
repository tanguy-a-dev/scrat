import { describe, expect, it } from "vitest";

import {
  buildCategoryBranches,
  categoryMatchesFilter,
  type CategoryDto,
} from "$lib/api";

function category(
  id: string,
  name: string,
  parent_id: string | null = null,
): CategoryDto {
  return { id, name, parent_id, icon: null, is_default: false };
}

const tree = [
  category("home", "Logement"),
  category("rent", "Loyer", "home"),
  category("power", "Électricité", "home"),
  category("food", "Alimentation"),
  category("shop", "Supermarché", "food"),
];

describe("buildCategoryBranches", () => {
  it("puts a parent's subcategories in its branch", () => {
    const branches = buildCategoryBranches(tree);

    expect(branches.get("home")).toEqual(new Set(["home", "rent", "power"]));
  });

  it("leaves a subcategory's branch as just itself", () => {
    const branches = buildCategoryBranches(tree);

    expect(branches.get("rent")).toEqual(new Set(["rent"]));
  });

  it("keeps sibling branches apart", () => {
    const branches = buildCategoryBranches(tree);

    expect(branches.get("food")).toEqual(new Set(["food", "shop"]));
  });

  /* A category naming a parent that isn't in the list still has to be
     selectable by its own id — dropping it would hide rows entirely rather
     than merely file them under the wrong parent, which is what SQL's
     subselect does with the same dangling reference. */
  it("keeps a category whose parent is missing from the list", () => {
    const branches = buildCategoryBranches([category("orphan", "Divers", "gone")]);

    expect(branches.get("orphan")).toEqual(new Set(["orphan"]));
  });
});

describe("categoryMatchesFilter", () => {
  const branches = buildCategoryBranches(tree);

  it("matches a row filed directly against the filtered category", () => {
    expect(categoryMatchesFilter(branches, "home", "home")).toBe(true);
  });

  /* The reason this function exists: filtering by a parent has to bring its
     subcategories' rows with it, the way the backend's own filter does and
     the way the header count over the list already reports. */
  it("matches a row filed against a subcategory of the filtered parent", () => {
    expect(categoryMatchesFilter(branches, "home", "rent")).toBe(true);
  });

  it("rejects a row from a different branch", () => {
    expect(categoryMatchesFilter(branches, "home", "shop")).toBe(false);
  });

  it("does not widen a subcategory filter back up to its parent", () => {
    expect(categoryMatchesFilter(branches, "rent", "home")).toBe(false);
  });

  it("falls back to an exact match when the categories aren't loaded yet", () => {
    const empty = new Map<string, Set<string>>();

    expect(categoryMatchesFilter(empty, "home", "home")).toBe(true);
    expect(categoryMatchesFilter(empty, "home", "rent")).toBe(false);
  });
});
