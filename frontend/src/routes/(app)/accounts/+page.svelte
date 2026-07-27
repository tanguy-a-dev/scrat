<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    formatMinorUnits,
    parseToMinorUnits,
    type AccountDto,
  } from "$lib/api";

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

  function handleToggleArchive(account: AccountDto) {
    if (account.status === "active") {
      withErrorHandling(() => api.archiveAccount(account.id));
    } else {
      withErrorHandling(() => api.activateAccount(account.id));
    }
  }

  function handleDelete(account: AccountDto) {
    if (!confirm(`Delete "${account.name}"? This cannot be undone.`)) return;
    withErrorHandling(() => api.deleteAccount(account.id));
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
      <li class="account" class:archived={account.status === "archived"}>
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
            >balance: {formatMinorUnits(account.balance_minor_units)}
            {account.currency}</span
          >
          <span class="status">{account.status}</span>
          <button
            type="button"
            onclick={() => handleToggleArchive(account)}
          >
            {account.status === "active" ? "Archive" : "Unarchive"}
          </button>
          <button type="button" class="danger" onclick={() => handleDelete(account)}>
            Delete
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
    color: #d33;
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
  button {
    border-radius: 6px;
    border: 1px solid rgba(0, 0, 0, 0.15);
    padding: 0.45rem 0.7rem;
    font-size: 0.95rem;
    font-family: inherit;
  }

  button {
    cursor: pointer;
    background-color: #396cd8;
    color: white;
    border: none;
  }

  button.danger {
    background-color: #b3261e;
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
    background-color: rgba(0, 0, 0, 0.03);
  }

  .account.archived {
    opacity: 0.6;
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

  .status {
    font-size: 0.8rem;
    text-transform: uppercase;
    opacity: 0.6;
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
    background-color: rgba(0, 0, 0, 0.08);
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

  @media (prefers-color-scheme: dark) {
    input,
    button:not(.danger) {
      border-color: rgba(255, 255, 255, 0.15);
    }

    input {
      background-color: rgba(255, 255, 255, 0.06);
      color: inherit;
    }

    .account {
      background-color: rgba(255, 255, 255, 0.05);
    }

    .chip {
      background-color: rgba(255, 255, 255, 0.12);
    }
  }
</style>
