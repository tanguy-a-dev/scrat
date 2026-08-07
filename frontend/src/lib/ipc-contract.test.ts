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
