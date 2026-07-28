<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    formatMinorUnits,
    formatCurrency,
    parseToMinorUnits,
    type AccountDto,
  } from "$lib/api";
  import { Trash2 } from "@lucide/svelte";

  let accounts = $state<AccountDto[]>([]);
  let loading = $state(true);
  let error = $state("");

  let newName = $state("");
  let newOpeningBalance = $state("0");
  let newPatternDrafts = $state<Record<string, string>>({});

  onMount(load);

  async function load() {
    loading = true;
    error = "";
    try {
      accounts = await api.listAccounts();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function withErrorHandling(action: () => Promise<unknown>) {
    error = "";
    try {
      await action();
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  function handleCreate(event: Event) {
    event.preventDefault();
    const minorUnits = parseToMinorUnits(newOpeningBalance || "0");
    if (minorUnits === null) {
      error = "Opening balance must be a number.";
      return;
    }
    withErrorHandling(async () => {
      await api.createAccount(newName, minorUnits);
      newName = "";
      newOpeningBalance = "0";
    });
  }

  function handleRename(account: AccountDto, name: string) {
    if (name.trim() === account.name) return;
    withErrorHandling(() => api.renameAccount(account.id, name));
  }

  function handleOpeningBalanceChange(account: AccountDto, raw: string) {
    const minorUnits = parseToMinorUnits(raw);
    if (minorUnits === null || minorUnits === account.opening_balance_minor_units)
      return;
    withErrorHandling(() => api.setOpeningBalance(account.id, minorUnits));
  }

  function handleAddPattern(account: AccountDto) {
    const pattern = (newPatternDrafts[account.id] ?? "").trim();
    if (!pattern) return;
    withErrorHandling(async () => {
      await api.addSourcePattern(account.id, pattern);
      newPatternDrafts[account.id] = "";
    });
  }

  function handleRemovePattern(account: AccountDto, pattern: string) {
    withErrorHandling(() => api.removeSourcePattern(account.id, pattern));
  }

  function handleDelete(account: AccountDto) {
    if (!confirm(`Delete "${account.name}"? This cannot be undone.`)) return;
    withErrorHandling(() => api.deleteAccount(account.id));
  }

  function handleSetDefault(account: AccountDto) {
    withErrorHandling(() => api.setDefaultAccount(account.id));
  }
</script>

<h1>Accounts</h1>

{#if error}
  <p class="error">{error}</p>
{/if}

<form class="create-form" onsubmit={handleCreate}>
  <input placeholder="Account name" bind:value={newName} required />
  <input
    type="number"
    step="0.01"
    placeholder="Opening balance"
    bind:value={newOpeningBalance}
  />
  <button type="submit">Add account</button>
</form>

{#if loading}
  <p>Loading…</p>
{:else if accounts.length === 0}
  <p class="empty">No accounts yet — add one above.</p>
{:else}
  <ul class="accounts">
    {#each accounts as account (account.id)}
      <li class="account">
        <div class="row">
          <input
            class="name"
            value={account.name}
            onchange={(e) => handleRename(account, e.currentTarget.value)}
          />
          <input
            class="balance"
            type="number"
            step="0.01"
            value={formatMinorUnits(account.opening_balance_minor_units)}
            onchange={(e) =>
              handleOpeningBalanceChange(account, e.currentTarget.value)}
          />
          <span class="computed"
            >balance: {formatCurrency(
              account.balance_minor_units,
              account.currency,
            )}</span
          >
          {#if account.is_default}
            <span class="default-badge">default</span>
          {:else}
            <button type="button" onclick={() => handleSetDefault(account)}>
              Set as default
            </button>
          {/if}
          <button
            type="button"
            class="icon-button danger"
            aria-label="Delete account"
            onclick={() => handleDelete(account)}
          >
            <Trash2 size={16} />
          </button>
        </div>
        <div class="patterns">
          {#each account.source_patterns as pattern (pattern)}
            <span class="chip">
              {pattern}
              <button
                type="button"
                onclick={() => handleRemovePattern(account, pattern)}
                aria-label="Remove pattern">×</button
              >
            </span>
          {/each}
          <input
            class="pattern-input"
            placeholder="Add source pattern…"
            value={newPatternDrafts[account.id] ?? ""}
            oninput={(e) =>
              (newPatternDrafts[account.id] = e.currentTarget.value)}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                handleAddPattern(account);
              }
            }}
          />
        </div>
      </li>
    {/each}
  </ul>
{/if}

<style>
  h1 {
    margin-top: 0;
  }

  .error {
    color: var(--color-danger);
  }

  .empty {
    opacity: 0.75;
  }

  .create-form {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1.5rem;
  }

  input,
  button:not(.icon-button) {
    border-radius: 6px;
    border: 1px solid var(--color-shade-3);
    padding: 0.45rem 0.7rem;
    font-size: 0.95rem;
    font-family: inherit;
  }

  input {
    background-color: var(--color-shade-2);
    color: inherit;
  }

  button:not(.icon-button) {
    cursor: pointer;
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
    border: none;
  }

  .default-badge {
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    padding: 0.2rem 0.5rem;
    border-radius: 999px;
    background-color: var(--color-box-accent-bg);
    color: var(--color-box-accent-text);
  }

  .accounts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .account {
    padding: 1rem;
    border-radius: 10px;
    background-color: var(--color-shade-2);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .name {
    font-weight: 600;
    min-width: 10rem;
  }

  .balance {
    width: 7rem;
  }

  .computed {
    opacity: 0.8;
    font-size: 0.9rem;
  }

  .patterns {
    margin-top: 0.6rem;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    background-color: var(--color-shade-3);
    border-radius: 999px;
    padding: 0.15rem 0.3rem 0.15rem 0.7rem;
    font-size: 0.85rem;
  }

  .chip button {
    background: none;
    color: inherit;
    border: none;
    padding: 0 0.3rem;
    font-size: 1rem;
    line-height: 1;
  }

  .pattern-input {
    flex: 1;
    min-width: 10rem;
  }
</style>
