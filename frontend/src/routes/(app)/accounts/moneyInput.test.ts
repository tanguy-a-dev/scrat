import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";

/* The API is mocked because a real `invoke` needs the Tauri bridge, which
   only exists inside the desktop shell — but the money formatting/parsing
   helpers exported from the same module are the real ones, since they are
   half of what's under test here. */
const listAccounts = vi.fn();
const establishOpeningBalance = vi.fn();
const reconcileAccount = vi.fn();
vi.mock("$lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/api")>();
  return {
    ...actual,
    api: {
      listAccounts: () => listAccounts(),
      listTransferRules: () => Promise.resolve([]),
      establishOpeningBalance: (...args: unknown[]) => establishOpeningBalance(...args),
      reconcileAccount: (...args: unknown[]) => reconcileAccount(...args),
    },
  };
});

const { i18n } = await import("$lib/i18n.svelte");
const Page = (await import("./+page.svelte")).default;

/* €1,234.56 on screen, anchored at €1,000.00 — so the ledger sum is €234.56,
   which is the number both previews are built out of. */
const account = {
  id: "acc-1",
  name: "Current",
  balance_minor_units: 123456,
  is_opening_balance_set: true,
  opening_balance_minor_units: 100000,
  has_transactions: true,
  currency: "EUR",
  description_patterns: [],
  is_default: true,
};

let target: HTMLElement;
let component: Record<string, unknown>;

async function render() {
  target = document.createElement("div");
  document.body.appendChild(target);
  component = mount(Page, { target });
  // `onMount(load)` is async; wait for the account list to reach the DOM.
  await vi.waitFor(() => expect(target.querySelector(".account")).not.toBeNull());
}

/** Opens a panel by the label on its trigger button. */
function openPanel(label: string) {
  const button = [...target.querySelectorAll("button")].find(
    (b) => b.textContent?.trim() === label,
  );
  expect(button, `no button labelled "${label}"`).toBeDefined();
  button!.click();
  flushSync();
}

function moneyInput(): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>('.reconcile input[id]');
  expect(input, "panel has no money input").not.toBeNull();
  return input!;
}

/** Types into the panel's money input the way a user does — which is the
   whole point of this suite: with `type="number"` the bound state arrives as
   a `number` (or `null` when the browser rejects the text), and every
   `parseToMinorUnits` call downstream throws on it. */
function type(text: string) {
  const input = moneyInput();
  input.value = text;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

/** The bottom line of the panel's preview — "Starting point becomes" or
    "Adjustment posted". */
function result(): string {
  return target.querySelector(".preview .result dd")?.textContent?.trim() ?? "";
}

beforeEach(() => {
  listAccounts.mockResolvedValue([account]);
  establishOpeningBalance.mockResolvedValue(undefined);
  reconcileAccount.mockResolvedValue(null);
});

afterEach(() => {
  if (component) unmount(component);
  target?.remove();
  i18n.setLanguage("en");
  vi.clearAllMocks();
});

describe("starting-point panel", () => {
  it("previews the anchor the typed balance works back to", async () => {
    await render();
    openPanel("Edit starting point");
    type("2000.00");
    // 2000.00 observed - 234.56 on record
    expect(result()).toBe("€1,765.44");
  });

  it("accepts a decimal comma, whatever the interface language", async () => {
    await render();
    openPanel("Edit starting point");
    type("1500,50");
    expect(result()).toBe("€1,265.94");
  });

  it("pre-fills with the balance in the language's own separators", async () => {
    i18n.setLanguage("fr");
    await render();
    openPanel("Modifier le point de départ");
    // A `type="number"` input rejects this outright and renders blank, which
    // is what left French users with an empty box and no preview.
    expect(moneyInput().value).toBe("1234,56");
    // Narrow no-break space: French groups thousands with U+202F, not a plain space.
    expect(result()).toBe("€1\u202f000,00");
  });

  it("sends the observed balance on Apply, not the anchor it previews", async () => {
    await render();
    openPanel("Edit starting point");
    type("2000.00");
    openPanel("Apply");
    await vi.waitFor(() => expect(establishOpeningBalance).toHaveBeenCalled());
    // The backend does the `observed - ledger sum` subtraction; the preview
    // only shows what it will land on.
    expect(establishOpeningBalance).toHaveBeenCalledWith("acc-1", 200000);
  });
});

describe("adjustment panel", () => {
  it("previews the entry that would be posted", async () => {
    await render();
    openPanel("Add adjustment");
    type("1300,00");
    // 1300.00 observed - 1234.56 the app believes
    expect(result()).toBe("+€65.44");
  });

  it("sends the observed balance on Apply, not the difference", async () => {
    await render();
    openPanel("Add adjustment");
    type("1300.00");
    openPanel("Apply");
    await vi.waitFor(() => expect(reconcileAccount).toHaveBeenCalled());
    expect(reconcileAccount.mock.calls[0][1]).toBe(130000);
  });
});
