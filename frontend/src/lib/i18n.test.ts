import { afterEach, describe, expect, it } from "vitest";

import { formatMinorUnits, formatMoney, parseToMinorUnits } from "./api";
import {
  DEFAULT_LANGUAGE,
  describeError,
  errorCode,
  i18n,
  isLanguage,
  monthNames,
  numberSeparators,
  t,
  tp,
} from "./i18n.svelte";

/* The language is one process-wide value, so every test that moves it has to
   put it back — otherwise the first test to switch to French silently decides
   the language for every test that runs after it. */
afterEach(() => {
  i18n.language = DEFAULT_LANGUAGE;
});

describe("t", () => {
  it("returns the message for the current language", () => {
    expect(t("nav.overview")).toBe("Overview");

    i18n.language = "fr";

    expect(t("nav.overview")).toBe("Vue d'ensemble");
  });

  it("interpolates named placeholders", () => {
    expect(t("unlock.tooShort", { min: 8 })).toBe(
      "Passphrase must be at least 8 characters.",
    );
  });

  /* A placeholder with no matching parameter is left as-is rather than
     replaced with "undefined" — a visible `{min}` reads as a bug to whoever
     sees it, which is what it is; "at least undefined characters" reads as a
     sentence and hides it. */
  it("leaves a placeholder alone when no value is supplied", () => {
    expect(t("unlock.tooShort")).toContain("{min}");
  });

  /* Runtime-built keys (`cadence.${x}`, `operationKind.${x}`) can name
     something no dictionary has, when a newer database carries a value this
     build predates. Showing the raw key beats throwing inside a render. */
  it("falls back to the key itself when the message is missing", () => {
    expect(t("operationKind.crypto_swap" as never)).toBe("operationKind.crypto_swap");
  });
});

describe("tp", () => {
  it("picks singular and plural in English on 1", () => {
    expect(tp("transactions.count", 1)).toBe("1 transaction");
    expect(tp("transactions.count", 2)).toBe("2 transactions");
    expect(tp("transactions.count", 0)).toBe("0 transactions");
  });

  /* French differs from English on exactly one count, and it is the one a
     "nothing happened" message hits most: zero is singular. Applying
     English's rule would print "0 transactions" where French wants
     "0 transaction". */
  it("treats zero as singular in French", () => {
    i18n.language = "fr";

    expect(tp("transactions.count", 0)).toBe("0 transaction");
    expect(tp("transactions.count", 1)).toBe("1 transaction");
    expect(tp("transactions.count", 2)).toBe("2 transactions");
  });
});

describe("describeError", () => {
  it("translates a backend code", () => {
    expect(describeError({ code: "incorrect_passphrase" })).toBe("Incorrect passphrase.");

    i18n.language = "fr";

    expect(describeError({ code: "incorrect_passphrase" })).toBe(
      "Phrase secrète incorrecte.",
    );
  });

  it("interpolates the params a code carries", () => {
    expect(
      describeError({ code: "account_has_transactions", params: { count: "12" } }),
    ).toContain("12");
  });

  /* An unrecognised code names itself. "Something went wrong" alone gives the
     user nothing to report and the maintainer nothing to act on. */
  it("names an unknown code rather than swallowing it", () => {
    expect(describeError({ code: "future_code" })).toContain("future_code");
  });

  /* Not everything thrown is a backend error — a TypeError from the missing
     Tauri bridge in a browser tab, or a bug in the frontend. Dressing those
     up as a friendly sentence would destroy the only diagnostic there is. */
  it("passes a non-backend throw through untouched", () => {
    expect(describeError(new TypeError("boom"))).toContain("boom");
    expect(describeError("plain string")).toBe("plain string");
  });
});

describe("errorCode", () => {
  /* Callers branch on this instead of matching English prose — the sniff that
     used to decide whether a category delete could offer a reassign prompt. */
  it("exposes the code for branching, and null for anything else", () => {
    expect(errorCode({ code: "category_requires_reassignment" })).toBe(
      "category_requires_reassignment",
    );
    expect(errorCode(new Error("nope"))).toBeNull();
    expect(errorCode(undefined)).toBeNull();
  });
});

describe("locale-dependent formatting", () => {
  it("names months in the interface language", () => {
    expect(monthNames()[0]).toBe("January");

    i18n.language = "fr";

    expect(monthNames()[0]).toBe("janvier");
  });

  /* `1,234` means a thousand to an English reader and one-and-a-bit to a
     French one, so this is a correctness question, not a cosmetic one. */
  it("groups and separates numbers the way the language does", () => {
    expect(formatMoney(10012323)).toBe("100,123.23");

    i18n.language = "fr";

    expect(formatMoney(10012323)).toBe(`100${numberSeparators().group}123,23`);
  });

  it("writes an editable amount with the language's decimal separator", () => {
    expect(formatMinorUnits(1234)).toBe("12.34");

    i18n.language = "fr";

    expect(formatMinorUnits(1234)).toBe("12,34");
  });
});

describe("parseToMinorUnits", () => {
  /* The bug this fixes: `parseFloat("12,34")` stops at the comma and returns
     12, so a French user typing on their numeric keypad silently lost the
     decimals. Both separators are accepted whatever the language, since the
     keyboard doesn't know what the app is set to. */
  it("accepts either decimal separator, in either language", () => {
    expect(parseToMinorUnits("12.34")).toBe(1234);
    expect(parseToMinorUnits("12,34")).toBe(1234);

    i18n.language = "fr";

    expect(parseToMinorUnits("12.34")).toBe(1234);
    expect(parseToMinorUnits("12,34")).toBe(1234);
  });

  it("tolerates the spaces a pasted grouped figure carries", () => {
    expect(parseToMinorUnits("1 234,50")).toBe(123450);
    expect(parseToMinorUnits("1 234,50")).toBe(123450);
  });

  /* Two separators have no safe reading — which one groups and which one
     divides depends on a convention this function cannot see. Guessing would
     turn "1,234.56" into either 1234.56 or 1.23, and being wrong about money
     by a factor of a thousand is worse than refusing. */
  it("refuses a figure carrying both separators rather than guessing", () => {
    expect(parseToMinorUnits("1,234.56")).toBeNull();
    expect(parseToMinorUnits("1.234,56")).toBeNull();
  });

  it("rejects text that is not a number at all", () => {
    expect(parseToMinorUnits("")).toBeNull();
    expect(parseToMinorUnits("abc")).toBeNull();
  });
});

describe("isLanguage", () => {
  /* Guards the value coming back from the database, which a hand-edited
     settings row or a newer build can make anything at all. */
  it("accepts only the tags this build speaks", () => {
    expect(isLanguage("en")).toBe(true);
    expect(isLanguage("fr")).toBe(true);
    expect(isLanguage("de")).toBe(false);
    expect(isLanguage("EN")).toBe(false);
    expect(isLanguage(null)).toBe(false);
  });
});
