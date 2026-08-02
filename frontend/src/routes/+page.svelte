<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { api } from "$lib/api";

  type Screen = "loading" | "create" | "unlock" | "fatal";

  let screen = $state<Screen>("loading");
  let passphrase = $state("");
  let confirmPassphrase = $state("");
  let error = $state("");
  let submitting = $state(false);
  let onePasswordConfigured = $state(false);
  let waitingFor1Password = $state(false);

  onMount(async () => {
    try {
      const initialized = await api.isDbInitialized();
      screen = initialized ? "unlock" : "create";
    } catch (e) {
      error = String(e);
      screen = "fatal";
      return;
    }
    if (screen !== "unlock") return;

    // A missing/unreadable config must never block manual unlock, so this
    // failing is silent — the passphrase form is already on screen.
    try {
      onePasswordConfigured = (await api.get1PasswordReference()) !== null;
    } catch {
      return;
    }
    if (onePasswordConfigured) await unlockVia1Password();
  });

  /** Triggers 1Password's own Touch ID prompt. On any failure we fall back
   * to the passphrase form rather than trapping the user — 1Password being
   * unavailable must never make the database unreachable. */
  async function unlockVia1Password() {
    error = "";
    waitingFor1Password = true;
    try {
      await api.unlockWith1Password();
      await goto("/overview");
    } catch (e) {
      error = String(e);
    } finally {
      waitingFor1Password = false;
    }
  }

  async function handleCreate(event: Event) {
    event.preventDefault();
    error = "";
    if (passphrase.length < 8) {
      error = "Passphrase must be at least 8 characters.";
      return;
    }
    if (passphrase !== confirmPassphrase) {
      error = "Passphrases do not match.";
      return;
    }
    submitting = true;
    try {
      await api.createDb(passphrase);
      await goto("/overview");
    } catch (e) {
      error = String(e);
    } finally {
      submitting = false;
    }
  }

  async function handleUnlock(event: Event) {
    event.preventDefault();
    error = "";
    if (!passphrase) {
      error = "Passphrase cannot be empty.";
      return;
    }
    submitting = true;
    try {
      await api.unlockDb(passphrase);
      await goto("/overview");
    } catch (e) {
      error = String(e);
    } finally {
      submitting = false;
    }
  }
</script>

<main class="container">
  <img src="/favicon.png" class="logo" alt="Scrat" />
  <h1>Scrat</h1>

  {#if screen === "loading"}
    <p class="subtitle">Loading…</p>
  {:else if screen === "fatal"}
    <p class="error">{error}</p>
  {:else if screen === "create"}
    <p class="subtitle">
      Set a passphrase to encrypt your local data. There is no recovery —
      if you lose it, your data is unreadable.
    </p>
    <form onsubmit={handleCreate}>
      <input
        type="password"
        id="new-passphrase"
        name="new-password"
        placeholder="Passphrase"
        bind:value={passphrase}
        autocomplete="new-password"
      />
      <input
        type="password"
        id="confirm-passphrase"
        name="confirm-password"
        placeholder="Confirm passphrase"
        bind:value={confirmPassphrase}
        autocomplete="new-password"
      />
      {#if error}<p class="error">{error}</p>{/if}
      <button type="submit" disabled={submitting}>
        Create encrypted database
      </button>
    </form>
  {:else if screen === "unlock"}
    <p class="subtitle">
      {waitingFor1Password
        ? "Waiting for 1Password…"
        : "Enter your passphrase to unlock your data."}
    </p>
    <form onsubmit={handleUnlock}>
      <input
        type="password"
        id="passphrase"
        name="password"
        placeholder="Passphrase"
        bind:value={passphrase}
        autocomplete="current-password"
        disabled={waitingFor1Password}
      />
      {#if error}<p class="error">{error}</p>{/if}
      <button type="submit" disabled={submitting || waitingFor1Password}>
        Unlock
      </button>
      {#if onePasswordConfigured}
        <button
          type="button"
          class="secondary"
          onclick={unlockVia1Password}
          disabled={submitting || waitingFor1Password}
        >
          {waitingFor1Password ? "Waiting…" : "Unlock with 1Password"}
        </button>
      {/if}
    </form>
  {/if}
</main>

<style>
  .container {
    margin: 0;
    padding-top: 10vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 0.5em;
  }

  .logo {
    height: 5em;
    image-rendering: pixelated;
    margin-bottom: 0.25em;
  }

  .subtitle {
    max-width: 28em;
    opacity: 0.8;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 0.6em;
    width: 20em;
    margin-top: 0.5em;
  }

  input,
  button {
    border-radius: 8px;
    border: 1px solid var(--color-shade-3);
    padding: 0.6em 1em;
    font-size: 1em;
    font-family: inherit;
  }

  input {
    background-color: var(--color-shade-2);
    color: inherit;
  }

  button {
    cursor: pointer;
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
    border: none;
  }

  button:disabled {
    opacity: 0.6;
    cursor: default;
  }

  button.secondary {
    background-color: transparent;
    color: inherit;
    border: 1px solid var(--color-shade-3);
  }

  input:disabled {
    opacity: 0.6;
  }

  .error {
    color: var(--color-danger);
    margin: 0;
  }
</style>
