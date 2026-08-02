/**
 * Per-session memory of a page's *view* state — which range is selected, which
 * filters are set, how far down the user had scrolled. A route component is
 * unmounted the moment you navigate away, taking all of its `$state` with it,
 * so without this every visit starts from the defaults: switch Details to
 * Year, glance at Categories, come back, and you're looking at Month again.
 *
 * Two deliberate limits:
 *
 * - **View state only, never data.** Pages still re-fetch from the database on
 *   mount. Caching rows here would mean coming back to a ledger that silently
 *   disagrees with a change made from another page.
 * - **In memory only, never on disk.** A description filter is user data, and the
 *   only place this app writes user data is the encrypted database. Losing
 *   these on quit is also the right behavior: a fresh launch should open on a
 *   predictable view, not on whatever was on screen a week ago.
 *
 * What comes back is a plain, non-reactive object. Pages keep their own
 * `$state` and mirror into it from an `$effect`, so nothing ever renders
 * straight out of the cache — this holds one page's state while that page is
 * unmounted, it isn't a store.
 */
const store = new Map<string, object>();

/** The cached view state for `key`, created from `initial()` on first use and
 * handed back as the same object on every later visit. */
export function pageViewState<T extends object>(key: string, initial: () => T): T {
  const existing = store.get(key);
  if (existing) return existing as T;
  const created = initial();
  store.set(key, created);
  return created;
}
