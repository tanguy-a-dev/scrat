<script lang="ts">
  import { onMount } from "svelte";
  import { api, formatCurrency, type AccountDto } from "$lib/api";

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
  let currency = $derived(activeAccounts[0]?.currency ?? "EUR");
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
      <span class="amount">{formatCurrency(total, currency)}</span>
    </div>
    {#each activeAccounts as account (account.id)}
      <div class="box">
        <span class="label">{account.name}</span>
        <span class="amount"
          >{formatCurrency(account.balance_minor_units, account.currency)}</span
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
    color: var(--color-danger);
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
    background-color: var(--color-box);
  }

  .box.total {
    background-color: var(--color-box-accent-bg);
    color: var(--color-box-accent-text);
  }

  .label {
    font-size: 0.85rem;
    opacity: 0.8;
  }

  .amount {
    font-size: 1.5rem;
    font-weight: 700;
  }
</style>
