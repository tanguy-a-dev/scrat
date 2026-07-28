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

  onMount(async () => {
    try {
      const initialized = await api.isDbInitialized();
      screen = initialized ? "unlock" : "create";
    } catch (e) {
      error = String(e);
      screen = "fatal";
    }
  });

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
    <p class="subtitle">Enter your passphrase to unlock your data.</p>
    <form onsubmit={handleUnlock}>
      <input
        type="password"
        id="passphrase"
        name="password"
        placeholder="Passphrase"
        bind:value={passphrase}
        autocomplete="current-password"
      />
      {#if error}<p class="error">{error}</p>{/if}
      <button type="submit" disabled={submitting}>Unlock</button>
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
    border: 1px solid #ccc;
    padding: 0.6em 1em;
    font-size: 1em;
    font-family: inherit;
  }

  button {
    cursor: pointer;
    background-color: #396cd8;
    color: white;
    border: none;
  }

  button:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .error {
    color: #d33;
    margin: 0;
  }

  @media (prefers-color-scheme: dark) {
    input {
      background-color: #1f1f1f98;
      border-color: #444;
      color: #f6f6f6;
    }
  }
</style>
