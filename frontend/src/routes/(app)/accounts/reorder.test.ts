import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";

/* Dragging a row is the one interaction on this page whose result is an
   index computation, and the classic way to get it wrong is invisible:
   splicing the moved item out shifts every later index by one, so a naive
   move lands one slot short whenever the drag goes downwards. These tests
   drag in both directions for that reason. */
const listAccounts = vi.fn();
const reorderAccounts = vi.fn();
vi.mock("$lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/api")>();
  return {
    ...actual,
    api: {
      listAccounts: () => listAccounts(),
      listTransferRules: () => Promise.resolve([]),
      reorderAccounts: (...args: unknown[]) => reorderAccounts(...args),
    },
  };
});

const Page = (await import("./+page.svelte")).default;

function account(id: string, name: string) {
  return {
    id,
    name,
    balance_minor_units: 0,
    is_opening_balance_set: true,
    opening_balance_minor_units: 0,
    has_transactions: false,
    currency: "EUR",
    description_patterns: [],
    is_default: false,
  };
}

let target: HTMLElement;
let component: Record<string, unknown>;

async function render() {
  target = document.createElement("div");
  document.body.appendChild(target);
  component = mount(Page, { target });
  await vi.waitFor(() => expect(target.querySelectorAll(".account").length).toBe(3));
}

/** Drags the row at `from` onto the row at `to`, the way the browser does:
    press the grip (which is what arms `draggable` on the whole card, so the
    native drag image is the box rather than the icon), then `dragstart` on
    the card and `drop` on the destination. jsdom has no `DragEvent`, which
    costs nothing here — the handlers key off the dragged id stashed on
    `dragstart`, not off `dataTransfer`. */
function drag(from: number, to: number) {
  const rows = [...target.querySelectorAll(".account")];
  rows[from]
    .querySelector(".drag-handle")!
    .dispatchEvent(new Event("mousedown", { bubbles: true }));
  rows[from].dispatchEvent(new Event("dragstart", { bubbles: true }));
  rows[to].dispatchEvent(new Event("drop", { bubbles: true, cancelable: true }));
}

/** Hovers a dragged row over another without dropping, so the insertion
    indicator can be inspected. */
function dragOver(from: number, to: number) {
  const rows = [...target.querySelectorAll(".account")];
  rows[from]
    .querySelector(".drag-handle")!
    .dispatchEvent(new Event("mousedown", { bubbles: true }));
  rows[from].dispatchEvent(new Event("dragstart", { bubbles: true }));
  rows[to].dispatchEvent(new Event("dragover", { bubbles: true, cancelable: true }));
  flushSync();
  return rows[to];
}

/** Ends the drag the way releasing outside the list does, so the next hover
    starts from a clean slate. */
function handleDragEndOn(index: number) {
  [...target.querySelectorAll(".account")][index].dispatchEvent(
    new Event("dragend", { bubbles: true }),
  );
  flushSync();
}

function namesOnScreen(): string[] {
  return [...target.querySelectorAll<HTMLInputElement>(".account .name")].map((i) => i.value);
}

beforeEach(() => {
  listAccounts.mockResolvedValue([
    account("a", "Alpha"),
    account("b", "Bravo"),
    account("c", "Charlie"),
  ]);
  reorderAccounts.mockResolvedValue(undefined);
});

afterEach(() => {
  if (component) unmount(component);
  target?.remove();
  vi.clearAllMocks();
});

describe("dragging an account to reorder", () => {
  it("moves a row down to the position it was dropped on", async () => {
    await render();

    drag(0, 2);

    await vi.waitFor(() => expect(reorderAccounts).toHaveBeenCalled());
    expect(reorderAccounts).toHaveBeenCalledWith(["b", "c", "a"]);
    expect(namesOnScreen()).toEqual(["Bravo", "Charlie", "Alpha"]);
  });

  it("moves a row up to the position it was dropped on", async () => {
    await render();

    drag(2, 0);

    await vi.waitFor(() => expect(reorderAccounts).toHaveBeenCalled());
    expect(reorderAccounts).toHaveBeenCalledWith(["c", "a", "b"]);
    expect(namesOnScreen()).toEqual(["Charlie", "Alpha", "Bravo"]);
  });

  /* Dropping a row on itself is the commonest accidental drag, and it must
     not cost a write — the list is already in that order. */
  it("does not persist anything when a row is dropped on itself", async () => {
    await render();

    drag(1, 1);

    expect(reorderAccounts).not.toHaveBeenCalled();
    expect(namesOnScreen()).toEqual(["Alpha", "Bravo", "Charlie"]);
  });

  /* The insertion line has to promise what the drop actually does. Dragging
     downwards lands the row after the target (the moved row is spliced out
     first, shifting everything below it up), so a line above the target would
     say the opposite. */
  it("marks the edge the row would actually land on", async () => {
    await render();

    expect(dragOver(0, 2).className).toContain("drag-over-below");

    handleDragEndOn(2);
    expect(dragOver(2, 0).className).toContain("drag-over-above");
  });

  /* The optimistic reorder has to be undone if the write fails, or the list
     keeps showing an order the database never accepted. */
  it("reloads the stored order when the write is rejected", async () => {
    reorderAccounts.mockRejectedValue({ code: "invalid_reorder" });
    await render();

    drag(0, 2);

    await vi.waitFor(() => expect(namesOnScreen()).toEqual(["Alpha", "Bravo", "Charlie"]));
  });
});
