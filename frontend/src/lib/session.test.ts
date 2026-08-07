import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/* Both of the session's collaborators are mocked: `$app/navigation` because a
   real `goto` needs a mounted router, and the API because a real `invoke`
   needs the Tauri bridge, which only exists inside the desktop shell. What's
   under test is the idle bookkeeping, not either of those. */
const goto = vi.fn();
vi.mock("$app/navigation", () => ({ goto: (...args: unknown[]) => goto(...args) }));

const getAutoLockMinutes = vi.fn();
const lockDb = vi.fn();
vi.mock("./api", () => ({
  api: {
    getAutoLockMinutes: () => getAutoLockMinutes(),
    lockDb: () => lockDb(),
  },
}));

const { session } = await import("./session.svelte");

const MINUTE_MS = 60_000;
/** Matches `IDLE_CHECK_INTERVAL_MS` in the module under test. */
const CHECK_INTERVAL_MS = 5_000;

/** Advances fake timers a tick at a time, letting the promise chain inside the
 * idle check settle — `#checkIdle` awaits `lockDb()` before it locks.
 *
 * Stopping the watch the moment the session locks is what `+layout.svelte`
 * does, via an `$effect` on `session.unlocked`. It's reproduced here because
 * the class genuinely depends on it: `#checkIdle` doesn't reset
 * `#lastActivity` after locking, so a watch left running would re-lock on
 * every subsequent tick. See `relocks_until_the_watch_is_stopped` below,
 * which pins that behaviour directly. */
async function idleFor(ms: number) {
  for (let elapsed = 0; elapsed < ms; elapsed += CHECK_INTERVAL_MS) {
    await vi.advanceTimersByTimeAsync(CHECK_INTERVAL_MS);
    if (!session.unlocked) session.stopIdleWatch();
  }
}

beforeEach(() => {
  vi.useFakeTimers();
  goto.mockReset();
  getAutoLockMinutes.mockReset().mockResolvedValue(10);
  lockDb.mockReset().mockResolvedValue(undefined);
  session.markLocked();
  session.autoLockMinutes = 10;
});

afterEach(() => {
  session.stopIdleWatch();
  vi.useRealTimers();
});

describe("markUnlocked", () => {
  it("reloads the auto-lock setting from the database being opened", async () => {
    getAutoLockMinutes.mockResolvedValue(3);

    await session.markUnlocked();

    expect(session.unlocked).toBe(true);
    expect(session.autoLockMinutes).toBe(3);
  });

  /* The setting lives in the per-database settings table, so importing a
     different database can change it — reading it once at startup would leave
     the timer running on the previous database's value. */
  it("picks up a different database's value on a later unlock", async () => {
    getAutoLockMinutes.mockResolvedValue(3);
    await session.markUnlocked();

    getAutoLockMinutes.mockResolvedValue(30);
    await session.markUnlocked();

    expect(session.autoLockMinutes).toBe(30);
  });

  /* A failed settings read must not block an unlock the user is actively
     completing — they've already given the right passphrase. */
  it("still unlocks when the setting cannot be read", async () => {
    getAutoLockMinutes.mockRejectedValue(new Error("database is locked"));

    await session.markUnlocked();

    expect(session.unlocked).toBe(true);
    expect(session.autoLockMinutes).toBe(10);
  });
});

describe("idle auto-lock", () => {
  it("locks the database once the idle period elapses", async () => {
    await session.markUnlocked();
    session.autoLockMinutes = 1;
    session.startIdleWatch();

    await idleFor(MINUTE_MS + CHECK_INTERVAL_MS);

    expect(lockDb).toHaveBeenCalledTimes(1);
    expect(session.unlocked).toBe(false);
  });

  /* Leaving the user on a page full of their finances after locking the
     database would show stale figures nobody can refresh. */
  it("sends the user back to the unlock screen", async () => {
    await session.markUnlocked();
    session.autoLockMinutes = 1;
    session.startIdleWatch();

    await idleFor(MINUTE_MS + CHECK_INTERVAL_MS);

    expect(goto).toHaveBeenCalledWith("/");
  });

  it("does not lock before the idle period is up", async () => {
    await session.markUnlocked();
    session.autoLockMinutes = 5;
    session.startIdleWatch();

    await idleFor(4 * MINUTE_MS);

    expect(lockDb).not.toHaveBeenCalled();
    expect(session.unlocked).toBe(true);
  });

  /* Activity is recorded by the listeners immediately; the interval only
     samples. Someone typing steadily must never be locked out mid-sentence. */
  it("restarts the countdown on user activity", async () => {
    await session.markUnlocked();
    session.autoLockMinutes = 1;
    session.startIdleWatch();

    await idleFor(50_000);
    window.dispatchEvent(new Event("keydown"));
    await idleFor(50_000);

    expect(lockDb).not.toHaveBeenCalled();

    await idleFor(MINUTE_MS);

    expect(lockDb).toHaveBeenCalledTimes(1);
  });

  it.each(["mousemove", "mousedown", "keydown", "wheel", "touchstart"])(
    "treats %s as activity",
    async (eventName) => {
      await session.markUnlocked();
      session.autoLockMinutes = 1;
      session.startIdleWatch();

      await idleFor(50_000);
      window.dispatchEvent(new Event(eventName));
      await idleFor(50_000);

      expect(lockDb).not.toHaveBeenCalled();
    },
  );

  /* "Never" is a real setting, and it has to mean never — a zero treated as
     "lock immediately" would be the worst possible misreading. */
  it("never locks when auto-lock is disabled", async () => {
    await session.markUnlocked();
    session.autoLockMinutes = 0;
    session.startIdleWatch();

    await idleFor(60 * MINUTE_MS);

    expect(lockDb).not.toHaveBeenCalled();
    expect(session.unlocked).toBe(true);
  });

  /* The interval keeps firing while the first lock is still awaiting its IPC
     round-trip. Without the re-entrancy guard each tick would fire another
     `lockDb` and another `goto`. */
  it("locks once even if the lock call is slow", async () => {
    let release: () => void = () => {};
    lockDb.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          release = () => resolve();
        }),
    );
    await session.markUnlocked();
    session.autoLockMinutes = 1;
    session.startIdleWatch();

    await idleFor(MINUTE_MS + 4 * CHECK_INTERVAL_MS);
    release();
    await vi.advanceTimersByTimeAsync(0);

    expect(lockDb).toHaveBeenCalledTimes(1);
  });

  /* If the lock IPC fails, the safe response is still to lock the UI — a
     session left unlocked because of an error is the failure mode that
     actually matters here. `#checkIdle` locks in a `finally`, so the rejection
     propagates out of the interval callback as an unhandled rejection; the
     handler below absorbs it so it doesn't fail the run. */
  it("locks the UI even when the lock call fails", async () => {
    const swallow = () => {};
    process.on("unhandledRejection", swallow);
    try {
      lockDb.mockRejectedValue(new Error("no connection"));
      await session.markUnlocked();
      session.autoLockMinutes = 1;
      session.startIdleWatch();

      await idleFor(MINUTE_MS + CHECK_INTERVAL_MS);

      expect(session.unlocked).toBe(false);
      expect(goto).toHaveBeenCalledWith("/");
    } finally {
      process.off("unhandledRejection", swallow);
    }
  });

  /* The class doesn't reset `#lastActivity` when it locks, so on its own it
     stays past the threshold and locks again on every tick. Nothing in the app
     hits this — `+layout.svelte` stops the watch as soon as `unlocked` flips
     false — but the dependency is real and undocumented in the class itself,
     so it's pinned here: a future caller that starts the watch without that
     effect gets a lock loop, not a one-shot lock. */
  it("re-locks on every tick if the watch is left running", async () => {
    await session.markUnlocked();
    session.autoLockMinutes = 1;
    session.startIdleWatch();

    for (let i = 0; i < 14; i++) {
      await vi.advanceTimersByTimeAsync(CHECK_INTERVAL_MS);
    }

    expect(lockDb.mock.calls.length).toBeGreaterThan(1);
  });
});

describe("stopIdleWatch", () => {
  it("stops the countdown", async () => {
    await session.markUnlocked();
    session.autoLockMinutes = 1;
    session.startIdleWatch();
    session.stopIdleWatch();

    await idleFor(10 * MINUTE_MS);

    expect(lockDb).not.toHaveBeenCalled();
  });

  /* Starting twice must not leave a second interval running — it would halve
     the effective idle time and survive a single `stopIdleWatch`. */
  it("is enough to stop a watch that was started twice", async () => {
    await session.markUnlocked();
    session.autoLockMinutes = 1;
    session.startIdleWatch();
    session.startIdleWatch();
    session.stopIdleWatch();

    await idleFor(10 * MINUTE_MS);

    expect(lockDb).not.toHaveBeenCalled();
  });
});
