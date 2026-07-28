# Scrat — principles for working in this repo

Scrat is a privacy-first personal finance desktop app (Rust + Tauri 2 + SvelteKit).
Everything below is load-bearing: it reflects decisions already made (often the hard
way, via a real bug or a real design conflict), not style preference. Read it before
making architectural changes.

## The one rule everything else serves

**Privacy first, no exceptions.** No network calls, no telemetry, no analytics, ever.
All data lives in one local encrypted SQLite file. Before adding *any* dependency,
ask whether it phones home. Before committing *any* file, ask whether it could
contain real user data (bank exports, database files) — see "Never commit real
financial data" below. When a feature could be built with or without leaving the
machine, always pick without.

## Architecture: hexagonal / DDD, dependency direction is not optional

```
crates/domain          — pure Rust. No I/O, no async, no framework deps.
crates/application     — use-cases. Depends only on domain's traits (ports).
crates/infra-sqlite     — SQLCipher adapter. Implements domain's repository ports.
crates/infra-csv        — CSV detection/parsing adapter. Implements nothing DB-side.
src-tauri               — composition root. The ONLY crate allowed to depend on
                          everything. Wires concrete infra into use-cases, exposes
                          #[tauri::command]s, owns DTOs.
frontend                — SvelteKit. Talks only to src-tauri commands via src/lib/api.ts.
```

Rules that follow from this, non-negotiably:

- `domain` never imports `infra-*` or `tauri`. Ports (traits) are *defined* in
  `domain::ports`, *implemented* in `infra-*`. This is Dependency Inversion — if
  you're tempted to import `rusqlite` into `domain` or `application`, stop; the
  fix is a new trait method on the port, not a leaked dependency.
- `application` services take `&'a dyn SomeRepository` (a borrow), not
  `Arc<dyn SomeRepository>`. This isn't arbitrary — the SQLite connection only
  lives as long as a `MutexGuard` held for the duration of one Tauri command, so
  every service is constructed fresh per command, per repository call site. Don't
  "fix" this into an owned `Arc` without solving that lifetime problem first.
  See `with_service` helpers in `src-tauri/src/*.rs` for the pattern.
- One narrow port per aggregate (`AccountRepository`, `CategoryRepository`,
  `TransactionRepository`), not one god-repository. A future importer (OFX, QIF)
  is a new `infra-*` crate implementing an existing port — it should never require
  editing `application` or `domain`.
- Referential checks that need repository access (does this category exist, does
  this account exist) belong in the **application** layer's use-cases, not in
  domain constructors. Domain constructors only enforce invariants they can check
  with the data already in hand.

## Domain modeling conventions

- **Money is always integer minor units (`i64`), never `f64`.** No exceptions —
  floating point on money is how you get off-by-a-cent bugs that compound.
- **No currency is stored per-transaction.** The app has one global currency
  setting (`settings.currency_code`), read at query time and applied uniformly.
  This is deliberate: changing the setting is a display-only relabel of every
  amount, past and future, with zero extra bookkeeping — because there's nothing
  to reconcile. If you ever add multi-currency support for real, this assumption
  breaks everywhere `Money::from_minor_units` is called with an app-wide
  currency; treat that as a major migration, not a small patch.
- **Account balance is `opening_balance` (a real stored field) + `SUM(transactions)`,
  not a cached running total.** The opening balance is a genuine anchor value the
  user sets once (or edits) — not a cache of the ledger, so it can't drift from it.
  Never add a second stored "current balance" field; it will drift.
- **Value objects validate in their constructor** (`AccountName::new`,
  `CategoryName::new`, `SourceText::new`, `Currency::new`, `Money`). If a value
  exists, it's already valid — don't re-validate at every call site.
- **IDs are UUID newtypes per aggregate** (`AccountId`, `CategoryId`,
  `TransactionId`), not raw `String`/`Uuid`. Don't collapse them into a shared
  `Id` type — that's exactly the kind of "DRY" that silently lets an
  `AccountId` be passed where a `CategoryId` is expected.
- **Categories are a strict two-level hierarchy**: a category may have one parent,
  but a category that already has children cannot itself become a child, and a
  subcategory cannot be given children. See `has_children` in
  `crates/domain/src/category.rs` and the `ParentIsSubcategory`/`HasSubcategories`
  errors in `CategoryService`. This replaced an earlier arbitrary-depth design
  with cycle detection — if you're reading old context (commit messages, an
  earlier design doc) that describes unlimited nesting, the code is the source
  of truth, not the history.
- **Deletes that would orphan data are refused, not cascaded.** Accounts/categories
  with transactions require an explicit archive or reassignment target — a
  finance app must never silently drop ledger history.

## Database

- SQLCipher via `rusqlite`, **pinned to `0.32`** (see `crates/infra-sqlite/Cargo.toml`).
  Don't bump to `0.38`+ without checking first — its `libsqlite3-sys` had a
  build-script that used an unstable `cfg_select!` feature and failed to compile
  on stable rustc at the time this was pinned. Re-verify before upgrading.
- Migrations are append-only numbered SQL files in
  `crates/infra-sqlite/src/migrations/`, tracked via a `schema_migrations` table.
  Never edit or renumber a shipped migration — add a new one.
- The passphrase is never persisted anywhere. It exists only in the live,
  already-keyed `rusqlite::Connection` held in Tauri-managed state for the
  process lifetime. Don't add a "remember my passphrase" feature that writes it
  to disk in any form, hashed or not.
- **Never commit real financial data.** `.gitignore` excludes `*.db`, `sample-data/`,
  and `T_cpte_*.csv` (a real bank export was used to validate the CSV importer
  during development — the pattern stays as a guard even though that file is
  gone). If you're given a real bank export to test against, use it for manual/
  interactive verification only; commit a *fabricated* fixture with the same
  structural quirks (ragged rows, decimal-comma amounts, shifting description
  column, etc.) instead. Before assuming any file is safe to commit, check
  `git log --all --full-history -- <path>` — don't take "it's gitignored now"
  as proof it wasn't committed earlier.

## Testing: TDD is the actual workflow, not a checkbox

Every layer has a distinct testing story — match new code to the existing one:

- **domain**: pure unit tests, no I/O, no mocks needed (`cargo test -p scrat-domain`).
- **application**: hand-written in-memory fake repositories implementing the
  port traits (see `FakeAccountRepository` etc. in each `*_service.rs`).
- **infra-sqlite**: real SQLCipher against a `tempfile` temp directory
  (`crate::create_new(&path, "test passphrase")`), not `:memory:` — the
  encryption keying behavior is part of what's under test.
- **infra-csv**: synthetic fixtures that mirror real bank-export messiness
  (ragged row lengths, decimal-comma amounts, no header) — see
  `detection.rs`'s test module for the pattern. Two real bugs were caught this
  way before the actual bank file did (delimiter sniffing fooled by a decimal
  separator; a sparse "flag" column tying the real amount column on a
  single-sample coincidence) — this style of test is pulling real weight, not
  theater.
- Before treating anything as "done": `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace` must all be clean. `make check` runs all three.

### Verifying UI changes

- `npm run check` (svelte-check) and `npm run build` must both pass clean.
- Then actually launch the app (`npm run tauri dev`, backgrounded, poll the log
  for `panicked`/`error`/`Finished`, confirm the process is alive) — a clean
  frontend build does not mean the Rust side didn't panic on first render.
- **The browser preview cannot exercise real app behavior.** `window.__TAURI_INTERNALS__`
  doesn't exist in a plain browser tab, so every `invoke()` call fails there by
  design — that's an expected `TypeError`, not a regression. It's still worth
  loading pages in the browser preview to confirm they render, theme correctly,
  and fail *gracefully* (no blank crash) without the Tauri bridge.
- To verify something that depends on real data flowing through the UI (e.g. the
  Details donut chart's math), temporarily replace a page's data-loading call
  with hardcoded mock data, screenshot/verify the computed result matches your
  own hand-calculation, then **revert the mock before committing** — don't ship
  debug scaffolding.
- If the browser preview shows a wall of `Failed to fetch dynamically imported
  module` console errors after several dev-server restarts in the same tab,
  that's stale-HMR-connection noise, not a real bug — open a fresh tab and
  re-check before chasing it.

## CSV import: heuristics need adversarial thinking, not just happy-path tests

The column-detection code (`crates/infra-csv/src/detection.rs`) has already hit
two classes of bug that are easy to reintroduce if this logic is touched:

1. **"It parsed" is not "it's plausible."** A long account/reference number
   parses fine as an integer and can overflow `i64` once converted to minor
   units, or silently coincide with `unwrap_or(0)` masking and score a
   coincidental win in column detection. Any numeric parsing of external input
   needs a plausibility ceiling and checked arithmetic, not just "does
   `.parse()` return `Ok`."
2. **Consistency alone isn't enough to pick a delimiter.** A decimal separator
   (`,` or `.`) can look like a perfectly consistent low-cardinality "delimiter."
   Weight delimiter scoring by how many fields it actually produces, not just
   how often it produces the same count.

If you change this file, add a fabricated-but-structurally-realistic regression
test for whatever broke, the same way the existing tests do.

## Frontend conventions

- Svelte 5 runes (`$state`, `$derived`, `$derived.by`) — this is not Svelte 4/
  Vue/React, don't reach for stores or class components.
- `frontend/src/lib/api.ts` is the single typed IPC boundary — every
  `#[tauri::command]` gets one typed wrapper function here. Don't call `invoke()`
  directly from a page component.
- No charting library. The Details donut is hand-rolled inline SVG (stacked
  `stroke-dasharray`/`stroke-dashoffset` circles) — this was a deliberate choice
  to avoid a dependency for one chart; don't add one without discussing it first.
- Shared UI pieces live in `frontend/src/lib/` (e.g. `CategoryNode.svelte`,
  `ImportCsvDialog.svelte`), not duplicated per-route.
- Theme (light/dark) lives in `frontend/src/app.css`, loaded once from the root
  `+layout.svelte`. Don't put `:root` color rules in a page-level `<style>`
  block — Svelte unmounts that stylesheet when you navigate away from the page,
  and the rest of the app silently loses its theme. (This exact bug happened
  once already.)

## Git conventions

- Conventional Commits (`feat:`, `fix:`, `chore:`, `style:`), with a body that
  explains *why*, not just what changed — future-you (or future-AI) reading
  `git log` should understand the reasoning without re-deriving it.
- Commits should be atomic per logical change. If a session's uncommitted work
  spans two unrelated features sharing a few touched files (e.g. two `mod`
  declarations added to the same `lib.rs`), it's worth splitting into two
  commits with intermediate file edits rather than one commit covering
  everything — each commit should compile and pass tests standalone.
- Never commit with `-A`/broad adds without reviewing `git status` first,
  especially given the real-financial-data risk above.

## What "done" means for a change here

1. It compiles across the whole workspace (`cargo build`), not just the crate
   you touched.
2. `cargo fmt`, `cargo clippy -D warnings`, and `cargo test --workspace` are
   clean.
3. If it touches the frontend: `npm run check` and `npm run build` are clean,
   and you've actually looked at it rendering (browser preview at minimum,
   native app launch for anything touching real data flow).
4. If it touches parsing of external/user-supplied input (CSV cells, file
   paths, anything not generated by this app itself): you've thought about the
   adversarial case, not just the sample you were handed.
