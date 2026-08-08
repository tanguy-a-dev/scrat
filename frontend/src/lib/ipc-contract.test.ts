import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

import { describe, expect, it } from "vitest";

/* The IPC boundary is the one contract in this app that nothing checks.
   `api.ts` names Rust commands as string literals: rename a `#[tauri::command]`
   and `cargo build` is happy, `svelte-check` is happy, `npm run build` is
   happy — and the app throws at runtime the first time a user clicks the
   thing. These tests read both sides and compare them.

   Reading the Rust source as text is deliberate. Anything smarter (a
   generated bindings file, a shared schema) would be a second artifact that
   can itself drift; the source of truth is the source. */

/* Vitest runs with its config root (`frontend/`) as cwd. `import.meta.url`
   would be cleaner, but under jsdom it isn't a `file:` URL. */
const REPO_ROOT = resolve(process.cwd(), "..");
const API_TS = join(REPO_ROOT, "frontend/src/lib/api.ts");
const TAURI_LIB_RS = join(REPO_ROOT, "src-tauri/src/lib.rs");
const FRONTEND_SRC = join(REPO_ROOT, "frontend/src");

/** Every command name `api.ts` passes to `invoke`. */
function invokedCommandNames(): string[] {
  const source = readFileSync(API_TS, "utf8");
  return [...source.matchAll(/invoke(?:<[^>]*>)?\(\s*"([a-z0-9_]+)"/g)].map((m) => m[1]);
}

/** Every command registered in `generate_handler!`, which is what actually
 * determines whether the frontend can reach it. A `#[tauri::command]` left out
 * of this list compiles fine and is still unreachable. */
function registeredCommandNames(): string[] {
  const source = readFileSync(TAURI_LIB_RS, "utf8");
  const block = source.match(/generate_handler!\[([\s\S]*?)\]/);
  if (!block) throw new Error(`no generate_handler! block found in ${TAURI_LIB_RS}`);
  return block[1]
    .split(",")
    .map((entry: string) => entry.trim())
    .filter(Boolean)
    .map((entry: string) => entry.split("::").pop()!);
}

function svelteAndTsFilesUnder(dir: string): string[] {
  const found: string[] = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      found.push(...svelteAndTsFilesUnder(path));
    } else if (/\.(svelte|ts)$/.test(entry) && !entry.endsWith(".test.ts")) {
      found.push(path);
    }
  }
  return found;
}

describe("the api.ts <-> Rust command contract", () => {
  const invoked = invokedCommandNames();
  const registered = registeredCommandNames();

  /* Guards the parsing itself: if either regex silently stops matching, the
     comparisons below would pass over two empty lists. */
  it("finds commands on both sides", () => {
    expect(invoked.length).toBeGreaterThan(20);
    expect(registered.length).toBeGreaterThan(20);
  });

  /* The failure this whole file exists for. A renamed command reaches the
     user as a runtime error on click, having passed every other check. */
  it("invokes only commands the Rust side registers", () => {
    const unregistered = invoked.filter((name) => !registered.includes(name));

    expect(unregistered).toEqual([]);
  });

  /* The other direction: a command nobody calls is either dead weight or a
     feature that was wired up on the backend and never connected. Either way
     it's worth knowing about rather than discovering years later. */
  it("registers only commands the frontend calls", () => {
    const uncalled = registered.filter((name) => !invoked.includes(name));

    expect(uncalled).toEqual([]);
  });

  it("registers each command exactly once", () => {
    expect(registered).toEqual([...new Set(registered)]);
  });
});

const ERRORS_RS = join(REPO_ROOT, "src-tauri/src/errors.rs");
const I18N_TS = join(REPO_ROOT, "frontend/src/lib/i18n.svelte.ts");

/** Every code the backend can put on the wire, read from the `codes` module. */
function backendErrorCodes(): string[] {
  const source = readFileSync(ERRORS_RS, "utf8");
  const block = source.match(/pub mod codes \{([\s\S]*?)\n\}/);
  if (!block) throw new Error(`no codes module found in ${ERRORS_RS}`);
  return [...block[1].matchAll(/pub const \w+: &str = "([a-z0-9_]+)";/g)].map((m) => m[1]);
}

/** Every `error.*` key the English dictionary defines. */
function dictionaryErrorKeys(): string[] {
  const source = readFileSync(I18N_TS, "utf8");
  const english = source.split("const en = {")[1]?.split("\n} as const;")[0];
  if (!english) throw new Error(`no English dictionary found in ${I18N_TS}`);
  return [...english.matchAll(/^\s*"error\.([a-z0-9_]+)":/gm)].map((m) => m[1]);
}

/* The second half of the same contract the command names get. Backend errors
   cross the wire as machine codes and the frontend dictionary owns the
   wording, so a code with no entry degrades to the generic "something went
   wrong" sentence — which compiles, type-checks, and reaches the user as a
   worse message than the one they used to get. */
describe("the error-code <-> dictionary contract", () => {
  const codes = backendErrorCodes();
  const keys = dictionaryErrorKeys();

  it("finds codes on both sides", () => {
    expect(codes.length).toBeGreaterThan(20);
    expect(keys.length).toBeGreaterThan(20);
  });

  it("has a message for every code the backend can emit", () => {
    const untranslated = codes.filter((code) => !keys.includes(code));

    expect(untranslated).toEqual([]);
  });

  /* The other direction: a message for a code nothing emits is dead weight,
     and usually the fossil of a renamed code whose replacement has no entry. */
  it("has no messages for codes the backend never emits", () => {
    // `unknown` is the frontend's own fallback for an unrecognised code, so
    // it deliberately has no counterpart in the Rust list.
    const unused = keys.filter((key) => key !== "unknown" && !codes.includes(key));

    expect(unused).toEqual([]);
  });
});

describe("the translation dictionaries", () => {
  /* `Record<MessageKey, string>` already makes a *missing* French key a
     compile error. What it cannot catch is a French entry that was pasted
     from the English block and never translated — those type-check
     perfectly. This is a coverage check, not a quality one: it only asserts
     the two dictionaries are the same size and shape. */
  it("defines the same keys in both languages", () => {
    const source = readFileSync(I18N_TS, "utf8");
    const keysOf = (block: string) =>
      [...block.matchAll(/^\s*"([\w.]+)":/gm)].map((m) => m[1]);
    const english = keysOf(source.split("const en = {")[1].split("\n} as const;")[0]);
    const french = keysOf(
      source.split("const fr: Record<MessageKey, string> = {")[1].split("\n};")[0],
    );

    expect(english.length).toBeGreaterThan(100);
    expect(french).toEqual(english);
  });
});

describe("the api.ts boundary", () => {
  /* `frontend/src/lib/api.ts` is the single typed IPC boundary — every command
     gets one typed wrapper there. A raw `invoke` in a page component skips the
     types, and skips the drift check above with them. */
  it("is the only module that calls invoke directly", () => {
    const offenders = svelteAndTsFilesUnder(FRONTEND_SRC)
      .filter((path) => path !== API_TS)
      .filter((path) => /\binvoke\s*(<[^>]*>)?\s*\(/.test(readFileSync(path, "utf8")))
      .map((path) => path.slice(REPO_ROOT.length));

    expect(offenders).toEqual([]);
  });

  it("imports invoke from the Tauri API rather than redefining it", () => {
    const source = readFileSync(API_TS, "utf8");

    expect(source).toMatch(/import\s*\{[^}]*\binvoke\b[^}]*\}\s*from\s*"@tauri-apps\/api\/core"/);
  });
});
