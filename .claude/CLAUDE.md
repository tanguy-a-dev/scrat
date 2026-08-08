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
  not a cached running total.** The opening balance is a genuine anchor value —
  not a cache of the ledger, so it can't drift from it. Never add a second
  stored "current balance" field; it will drift.
- **The opening balance is `Option<Money>`, and the UI never asks for it
  directly.** `None` means "nobody has established a starting point yet" and is
  *not* the same as zero — a user who says the account began empty has answered
  the question. Migration 0008's `opening_balance_set` column carries that
  distinction, because the amount column alone can't (both store 0). The reason
  it's never asked for: nobody can compute it. After importing a bank export it
  is `observed balance today - SUM(imported transactions)`, so the app asks for
  the one number the user can read off their bank and back-solves in
  `AccountService::establish_opening_balance`. Anything creating an `Account`
  from user input should use `Account::without_opening_balance`.
- **Setting the starting point and reconciling take the same input and mean
  opposite things — don't merge them.** `establish_opening_balance` shifts the
  undated anchor and writes no ledger row: it says "my records don't reach back
  far enough", and fixes every historical balance at once.
  `TransactionService::reconcile_account` posts a dated `Adjustment`: it says
  "money moved after my records begin that I never imported", and leaves the
  past alone. Using either for the other's job silently falsifies history —
  reconciling a bad starting point leaves every past balance wrong, and
  re-anchoring to absorb real drift back-dates money the account never had.
- **Value objects validate in their constructor** (`AccountName::new`,
  `CategoryName::new`, `Description::new`, `Currency::new`, `Money`). If a value
  exists, it's already valid — don't re-validate at every call site.
- **"Description" is the raw text a bank export carries for a row; "source
  account" is where a transfer's money leaves from.** These are different
  things and the word `source` used to mean both (plus a restore-from file
  path in `settings.rs`, which still legitimately uses it). Don't reintroduce
  `source` as a name for transaction text — `Description` / `description_*`
  is the domain word, and `merchant_key` in `recurring.rs` is the normalized
  form derived from it.
- **`Direction` (Income/Expense) is derived from the amount's sign;
  `TransactionRole` (Normal/Transfer/Adjustment) is stored.** Only `Normal`
  counts toward income/expense reporting — every role counts toward account
  balances. Keep the two concepts apart; they are not interchangeable
  classifications despite both sounding like "what kind of transaction".
- **`TransactionFingerprint` identifies, it does not deduplicate.** Migration
  0004 deliberately dropped its UNIQUE constraint — identical transactions are
  legitimate. Nothing rejects a write because the fingerprint already exists;
  it's a candidate key for a future "find likely duplicates" review feature.
- **IDs are UUID newtypes per aggregate** (`AccountId`, `CategoryId`,
  `TransactionId`), not raw `String`/`Uuid`. Don't collapse them into a shared
  `Id` type — that's exactly the kind of "DRY" that silently lets an
  `AccountId` be passed where a `CategoryId` is expected.
- **Categories are a strict two-level hierarchy**: a category may have one parent,
  but a category that already has children cannot itself become a child, and a
  subcategory cannot be given children. See `has_subcategories` in
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

## Interface language (English / French)

The app ships in two languages, chosen in Settings. Four decisions carry the
whole design; changing any of them touches every layer.

- **No user-visible English lives in Rust.** Tauri commands return an
  `AppError { code, params }` (`src-tauri/src/errors.rs`), never prose. The
  frontend dictionary owns every sentence. A code is part of the IPC contract
  exactly like a command name — `ipc-contract.test.ts` fails the build if a
  code has no message, or a message no code.
- **Branch on codes, never on message text.** `errorCode(e)` exists for the
  handful of places that need to know *which* failure happened. The categories
  page used to do `message.includes("reassign")`, which worked in English and
  would have silently stopped working in French, downgrading a recoverable
  prompt to a dead-end toast. If you find yourself matching on wording, that's
  the bug.
- **The translation layer is hand-rolled** (`frontend/src/lib/i18n.svelte.ts`),
  same call as the hand-rolled donut chart. `t(key, params)` reads a `$state`
  language, so every `t()` in markup re-renders on a language change with no
  subscription and no reload. `fr` is typed `Record<MessageKey, string>`, so a
  key added to `en` and forgotten in `fr` fails `npm run check`. Plurals go
  through `tp` — English and French disagree on zero (*0 catégorie*), and
  hardcoding English's rule is wrong on the count "nothing happened" messages
  hit most.
- **Locale formatting is written out, not taken from `Intl`.** `Intl` reads the
  *host* locale, so a French user on an English macOS would get English month
  names inside a French interface. Month/weekday names and number separators
  come from the language setting alone. `1,234` means a thousand in English and
  one-and-a-bit in French, so this is correctness, not decoration —
  `parseToMinorUnits` accordingly accepts both separators whatever the setting.

### Default categories and `seed_key`

Seeded categories are app-owned until the user touches them, and theirs after.
`categories.seed_key` (migration 0010, backfilled by English name) is what
tells those two states apart across a rename.

- A language change relabels a seeded category **only if its name is still
  character-for-character what the app wrote** (`relabel_seeded_categories`).
  Anything renamed, re-cased, or user-created is left alone forever. Getting
  this backwards destroys user-chosen names irreversibly; leaving a stale name
  is merely untidy and fixable in one edit.
- The catalogue lives in `crates/domain/src/default_categories.rs`, not in
  `infra-sqlite::seed`, because the application layer needs it to relabel and
  cannot depend on an adapter. **Seed keys are storage identifiers — never
  reword or renumber a shipped one**; adding entries is fine.
- The forced fallback category is found by `seed_key == "uncategorized"`, not
  by name. Keying its rename/delete protection off the English name would have
  let a French user delete the one category the whole app falls back to.
- A new database is always seeded in `Language::default()` — the language
  setting lives inside the database being created, so it cannot yet say
  otherwise. Switching afterwards is what the relabel pass is for.

## Frontend conventions

- Svelte 5 runes (`$state`, `$derived`, `$derived.by`) — this is not Svelte 4/
  Vue/React, don't reach for stores or class components.
- **Never name a local `t`.** It's the translation function, imported almost
  everywhere. The transactions page used `t` for its row variable and for a
  `handleDelete` parameter; both shadowed the import and had to be renamed.
- `frontend/src/lib/api.ts` is the single typed IPC boundary — every
  `#[tauri::command]` gets one typed wrapper function here. Don't call `invoke()`
  directly from a page component.
- No charting library. The Details donut is hand-rolled inline SVG (stacked
  `stroke-dasharray`/`stroke-dashoffset` circles) — this was a deliberate choice
  to avoid a dependency for one chart; don't add one without discussing it first.
- Shared UI pieces live in `frontend/src/lib/` (e.g. `CategoryCard.svelte`,
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

## Versioning and releases

Commit messages are now load-bearing: `.github/workflows/tag.yml` decides
the next version by parsing them, so a mistyped prefix silently changes what
ships. See the table in README.md for the type → bump mapping.

Tagging and building/releasing are two separate workflows, deliberately:
`tag.yml` runs automatically after every green CI run on `main` and, if
anything releasable landed, bumps the version and pushes a tag — cheap and
safe to do unconditionally. `build-release.yml` is `workflow_dispatch`-only:
it builds installers for macOS (Apple Silicon + Intel), Linux, and Windows for
a given tag (defaulting to the most recent one) and publishes the GitHub
release. Building three platforms' worth of binaries is not something that
should happen unattended on every commit, so it stays a manual, explicit
action even though tagging isn't. Don't merge these back into one workflow —
that was the previous design and it's what this split replaced.

- **The version lives in exactly one place**: `[workspace.package] version` in
  the root `Cargo.toml`. `src-tauri/Cargo.toml` inherits it with
  `version.workspace = true`, and `src-tauri/tauri.conf.json` has **no**
  `version` key at all so Tauri falls back to the Cargo one (verified in
  `tauri-codegen`'s `context.rs`: absent config version → `CARGO_PKG_VERSION`).
  Don't add it back "for clarity" — it's the same drift trap as a cached
  balance field. `scripts/set-version.sh` rewrites the two `package.json`s and
  refreshes `Cargo.lock` from that one value.
- **`scripts/next-version.sh` maps commit type → bump, and anything it doesn't
  recognise releases nothing.** That's deliberate: a typo'd prefix must never
  guess a version. Scrat's vocabulary is wider than the Conventional Commits
  spec (`ux:`, `ops:`, `refacto:`, `clean:`, `tests:`), so **adding a new
  commit prefix to the repo's vocabulary means adding it to that script's
  `bump_for_type` too**, or commits using it will never trigger a release.
- **Below 1.0.0 a breaking change bumps the minor, not the major.** Reaching
  1.0.0 should be a deliberate statement about stability, not a side effect of
  a `!` in a commit subject.
- `tag.yml` pushes a `chore(release): vX.Y.Z` commit back to main as part of
  tagging. It's filtered out of both the bump classification and the release
  notes — without that filter every release would beget another.
  `scripts/next-version-test.sh` covers this case; run it with
  `make release-test` (part of `make check`).
- `build-release.yml` checks out the tag itself (already carrying the bumped
  version from `tag.yml`'s commit), so it never re-stamps a version. Its
  release-notes step can't rely on plain `git describe` to find the previous
  release — the tag it's building already exists by the time it runs, so
  `git describe` on that commit resolves to itself. It walks to the tagged
  commit's parent first to find the actual previous tag. If you touch that
  logic, keep it working from an *existing* tag, not from a not-yet-tagged
  commit the way the old single-workflow version could assume.

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
