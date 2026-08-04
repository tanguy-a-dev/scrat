import { goto } from "$app/navigation";
import { api } from "./api";

const ACTIVITY_EVENTS = [
  "mousemove",
  "mousedown",
  "keydown",
  "wheel",
  "touchstart",
] as const;

/** How often idleness is checked, not how quickly it reacts to activity —
 * activity itself is recorded immediately via the listeners below. Coarse on
 * purpose: a `setInterval` tick is far cheaper than resetting a `setTimeout`
 * on every `mousemove`. */
const IDLE_CHECK_INTERVAL_MS = 5_000;

/** Tracks whether the database is currently unlocked and, while it is, locks
 * it after `autoLockMinutes` of no mouse/keyboard activity. `0` disables the
 * timer ("never"). There is exactly one instance of this for the app's
 * lifetime — it is not per-page state. */
class Session {
  unlocked = $state(false);
  autoLockMinutes = $state(10);

  #lastActivity = Date.now();
  #intervalId: ReturnType<typeof setInterval> | null = null;
  #locking = false;

  #recordActivity = () => {
    this.#lastActivity = Date.now();
  };

  /** Call once the passphrase has just been accepted (creation, unlock, or a
   * database import that leaves the new database open). Reloads the
   * auto-lock setting, since it lives in the per-database settings table and
   * a different database may have a different value. */
  async markUnlocked() {
    try {
      this.autoLockMinutes = await api.getAutoLockMinutes();
    } catch {
      // Keep whatever value was already held; a failed read shouldn't block
      // the unlock the user is actively completing.
    }
    this.unlocked = true;
  }

  markLocked() {
    this.unlocked = false;
  }

  startIdleWatch() {
    if (this.#intervalId) return;
    this.#lastActivity = Date.now();
    for (const event of ACTIVITY_EVENTS) {
      window.addEventListener(event, this.#recordActivity, { passive: true });
    }
    this.#intervalId = setInterval(() => this.#checkIdle(), IDLE_CHECK_INTERVAL_MS);
  }

  stopIdleWatch() {
    if (this.#intervalId) {
      clearInterval(this.#intervalId);
      this.#intervalId = null;
    }
    for (const event of ACTIVITY_EVENTS) {
      window.removeEventListener(event, this.#recordActivity);
    }
  }

  async #checkIdle() {
    if (this.#locking || this.autoLockMinutes <= 0) return;
    const idleMs = Date.now() - this.#lastActivity;
    if (idleMs < this.autoLockMinutes * 60_000) return;

    this.#locking = true;
    try {
      await api.lockDb();
    } finally {
      this.markLocked();
      this.#locking = false;
      await goto("/");
    }
  }
}

export const session = new Session();
