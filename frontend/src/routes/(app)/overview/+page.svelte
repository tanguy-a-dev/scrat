<script lang="ts">
  import { onMount } from "svelte";
  import { api, formatMinorUnits, type AccountDto } from "$lib/api";

  let accounts = $state<AccountDto[]>([]);
  let loading = $state(true);
  let error = $state("");

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

  let activeAccounts = $derived(accounts.filter((a) => a.status === "active"));
  let currency = $derived(activeAccounts[0]?.currency ?? "USD");
  let total = $derived(
    activeAccounts.reduce((sum, a) => sum + a.balance_minor_units, 0),
  );
</script>

<h1>Overview</h1>

{#if loading}
  <p>Loading…</p>
{:else if error}
  <p class="error">{error}</p>
{:else if activeAccounts.length === 0}
  <p class="empty">
    No accounts yet. Head to <a href="/accounts">Accounts</a> to add one.
  </p>
{:else}
  <div class="grid">
    <div class="box total">
      <span class="label">Total available</span>
      <span class="amount">{formatMinorUnits(total)} {currency}</span>
    </div>
    {#each activeAccounts as account (account.id)}
      <div class="box">
        <span class="label">{account.name}</span>
        <span class="amount"
          >{formatMinorUnits(account.balance_minor_units)} {account.currency}</span
        >
      </div>
    {/each}
  </div>
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

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
    gap: 1rem;
  }

  .box {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 1.25rem;
    border-radius: 10px;
    background-color: rgba(0, 0, 0, 0.04);
  }

  .box.total {
    background-color: #396cd8;
    color: white;
  }

  .label {
    font-size: 0.85rem;
    opacity: 0.8;
  }

  .amount {
    font-size: 1.5rem;
    font-weight: 700;
  }

  @media (prefers-color-scheme: dark) {
    .box {
      background-color: rgba(255, 255, 255, 0.06);
    }
  }
</style>
