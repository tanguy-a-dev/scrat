import { describe, expect, it } from "vitest";

import { buildCategoryOptions, type CategoryDto } from "$lib/api";

function category(
  id: string,
  name: string,
  parent_id: string | null = null,
): CategoryDto {
  return { id, name, parent_id, icon: null, is_default: false };
}

describe("buildCategoryOptions", () => {
  it("joins a subcategory with its parent and carries both levels apart", () => {
    const options = buildCategoryOptions([
      category("food", "Alimentation"),
      category("shop", "Supermarché", "food"),
    ]);

    expect(options).toEqual([
      { id: "food", label: "Alimentation", parentName: undefined, name: "Alimentation" },
      {
        id: "shop",
        label: "Alimentation > Supermarché",
        parentName: "Alimentation",
        name: "Supermarché",
      },
    ]);
  });

  it("leaves a top-level category without a parent name", () => {
    const [option] = buildCategoryOptions([category("pay", "Salaire")]);

    expect(option.parentName).toBeUndefined();
    expect(option.name).toBe("Salaire");
    expect(option.label).toBe("Salaire");
  });

  /* The reason `parentName`/`name` exist at all rather than being recovered
     from `label` by splitting on " > ". `CategoryName` only rejects empty and
     over-long names, so this is a category a user can really create — and a
     renderer that split the label would show it as a "Sport" subcategory
     called "Fitness", inventing a parent that doesn't exist. */
  it("keeps a category whose own name contains the separator in one piece", () => {
    const [option] = buildCategoryOptions([category("x", "Sport > Fitness")]);

    expect(option.name).toBe("Sport > Fitness");
    expect(option.parentName).toBeUndefined();
  });

  it("nests each subcategory directly under its own parent", () => {
    const options = buildCategoryOptions([
      category("home", "Logement"),
      category("food", "Alimentation"),
      category("rent", "Loyer", "home"),
      category("shop", "Supermarché", "food"),
    ]);

    expect(options.map((o) => o.label)).toEqual([
      "Logement",
      "Logement > Loyer",
      "Alimentation",
      "Alimentation > Supermarché",
    ]);
  });
});
