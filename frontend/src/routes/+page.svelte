<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import { session } from "$lib/session.svelte";
  import { describeError, i18n, t } from "$lib/i18n.svelte";

  type Screen = "loading" | "create" | "unlock" | "fatal";

  let screen = $state<Screen>("loading");
  let passphrase = $state("");
  let confirmPassphrase = $state("");
  let error = $state("");
  let submitting = $state(false);

  /** The minimum the backend enforces (`MIN_PASSPHRASE_LENGTH` in
   * `src-tauri/src/db.rs`). Repeated here only so the form can say so before
   * a round trip — the backend check is the one that counts. */
  const MIN_PASSPHRASE_LENGTH = 8;

  onMount(async () => {
    // There is no database open yet, so the language can only come from the
    // cache. Whatever this database actually says replaces it on unlock.
    i18n.restoreCached();
    try {
      const initialized = await api.isDbInitialized();
      screen = initialized ? "unlock" : "create";
    } catch (e) {
      error = describeError(e);
      screen = "fatal";
    }
  });

  async function handleCreate(event: Event) {
    event.preventDefault();
    error = "";
    if (passphrase.length < MIN_PASSPHRASE_LENGTH) {
      error = t("unlock.tooShort", { min: MIN_PASSPHRASE_LENGTH });
      return;
    }
    if (passphrase !== confirmPassphrase) {
      error = t("unlock.mismatch");
      return;
    }
    submitting = true;
    try {
      await api.createDb(passphrase);
      await session.markUnlocked();
      await goto("/overview");
    } catch (e) {
      error = describeError(e);
    } finally {
      submitting = false;
    }
  }

  async function handleUnlock(event: Event) {
    event.preventDefault();
    error = "";
    if (!passphrase) {
      error = t("unlock.empty");
      return;
    }
    submitting = true;
    try {
      await api.unlockDb(passphrase);
      await session.markUnlocked();
      await goto("/overview");
    } catch (e) {
      error = describeError(e);
    } finally {
      submitting = false;
    }
  }
</script>

<main class="container">
  <img src="/favicon.png" class="logo" alt="Scrat" />
  <h1>Scrat</h1>

  {#if screen === "loading"}
    <p class="subtitle">{t("common.loading")}</p>
  {:else if screen === "fatal"}
    <p class="error">{error}</p>
  {:else if screen === "create"}
    <p class="subtitle">{t("unlock.tagline")}</p>
    <form onsubmit={handleCreate}>
      <input
        type="password"
        id="new-passphrase"
        name="new-password"
        placeholder={t("unlock.passphrase")}
        bind:value={passphrase}
        autocomplete="new-password"
      />
      <input
        type="password"
        id="confirm-passphrase"
        name="confirm-password"
        placeholder={t("unlock.confirmPassphrase")}
        bind:value={confirmPassphrase}
        autocomplete="new-password"
      />
      {#if error}<p class="error">{error}</p>{/if}
      <button type="submit" disabled={submitting}>
        {t("unlock.create")}
      </button>
    </form>
  {:else if screen === "unlock"}
    <p class="subtitle">{t("unlock.enterPassphrase")}</p>
    <form onsubmit={handleUnlock}>
      <input
        type="password"
        id="passphrase"
        name="password"
        placeholder={t("unlock.passphrase")}
        bind:value={passphrase}
        autocomplete="current-password"
      />
      {#if error}<p class="error">{error}</p>{/if}
      <button type="submit" disabled={submitting}>{t("unlock.unlock")}</button>
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

  input:focus {
    outline: none;
    border-color: var(--color-accent);
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

  .error {
    color: var(--color-danger);
    margin: 0;
  }
</style>
