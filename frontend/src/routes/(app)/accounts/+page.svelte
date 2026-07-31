<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    formatMinorUnits,
    formatCurrency,
    parseToMinorUnits,
    todayIsoDate,
    type AccountDto,
    type TransferRuleDto,
  } from "$lib/api";
  import DeleteButton from "$lib/DeleteButton.svelte";
  import { toast } from "$lib/toasts.svelte";

  let accounts = $state<AccountDto[]>([]);
  let transferRules = $state<TransferRuleDto[]>([]);
  let loading = $state(true);
  let error = $state("");

  let newName = $state("");
  let newOpeningBalance = $state("0");
  let newPatternDrafts = $state<Record<string, string>>({});
  let newTransferPatternDrafts = $state<Record<string, string>>({});
  /** Which account has its reconcile input open, and what's typed in it.
   * Only one at a time — reconciling is a deliberate act against a number
   * read off another app, not something to have half-open everywhere. */
  let reconcilingAccountId = $state<string | null>(null);
  let reconcileDraft = $state("");
  /** Which account's "apply to past transactions" confirm is open. Only one
   * at a time, same reasoning as `reconcilingAccountId`. */
  let applyingRulesAccountId = $state<string | null>(null);

  onMount(load);

  async function load() {
    loading = true;
    error = "";
    try {
      const [a, r] = await Promise.all([
        api.listAccounts(),
        api.listTransferRules(),
      ]);
      accounts = a;
      transferRules = r;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  /** Rules that send money *to* this account — read on the card as "money
   * arriving here is recognized by these patterns". */
  function rulesFor(accountId: string): TransferRuleDto[] {
    return transferRules.filter((r) => r.counterpart_account_id === accountId);
  }

  async function withErrorHandling(action: () => Promise<unknown>) {
    try {
      await action();
      await load();
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function withDeleteConfirmation(
    action: () => Promise<unknown>,
    successMessage: string,
  ) {
    try {
      await action();
      await load();
      toast.success(successMessage);
    } catch (e) {
      toast.error(String(e));
    }
  }

  function handleCreate(event: Event) {
    event.preventDefault();
    const minorUnits = parseToMinorUnits(newOpeningBalance || "0");
    if (minorUnits === null) {
      toast.error("Opening balance must be a number.");
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
    withDeleteConfirmation(
      () => api.deleteAccount(account.id),
      `"${account.name}" deleted.`,
    );
  }

  function handleSetDefault(account: AccountDto) {
    withErrorHandling(() => api.setDefaultAccount(account.id));
  }

  function handleAddTransferRule(account: AccountDto) {
    const pattern = (newTransferPatternDrafts[account.id] ?? "").trim();
    if (!pattern) return;
    withErrorHandling(async () => {
      await api.createTransferRule(pattern, account.id);
      newTransferPatternDrafts[account.id] = "";
    });
  }

  function handleRemoveTransferRule(rule: TransferRuleDto) {
    withErrorHandling(() => api.deleteTransferRule(rule.id));
  }

  function startApplyTransferRules(account: AccountDto) {
    applyingRulesAccountId = account.id;
  }

  function cancelApplyTransferRules() {
    applyingRulesAccountId = null;
  }

  async function handleApplyTransferRules(account: AccountDto) {
    try {
      const summary = await api.applyTransferRules(account.id);
      applyingRulesAccountId = null;
      await load();
      toast.success(
        summary.converted > 0
          ? `${summary.converted} existing transaction(s) converted to transfers.`
          : "No matching transactions found.",
      );
    } catch (e) {
      toast.error(String(e));
    }
  }

  function startReconcile(account: AccountDto) {
    reconcilingAccountId = account.id;
    // Pre-filled with what the app currently believes, so the user edits a
    // number rather than typing one from scratch — and so submitting
    // unchanged is a visible no-op instead of an accident.
    reconcileDraft = formatMinorUnits(account.balance_minor_units);
  }

  function cancelReconcile() {
    reconcilingAccountId = null;
    reconcileDraft = "";
  }

  async function handleReconcile(account: AccountDto) {
    const minorUnits = parseToMinorUnits(reconcileDraft);
    if (minorUnits === null) {
      toast.error("Balance must be a number.");
      return;
    }
    try {
      const adjustment = await api.reconcileAccount(
        account.id,
        minorUnits,
        todayIsoDate(),
      );
      cancelReconcile();
      await load();
      toast.success(
        adjustment
          ? `Adjusted by ${formatCurrency(adjustment.amount_minor_units, account.currency)}.`
          : `"${account.name}" already matched — nothing to adjust.`,
      );
    } catch (e) {
      toast.error(String(e));
    }
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
          <button type="button" onclick={() => startReconcile(account)}>
            Reconcile
          </button>
          <DeleteButton
            label="Delete account"
            onConfirm={() => handleDelete(account)}
          />
        </div>
        {#if reconcilingAccountId === account.id}
          <div class="reconcile">
            <label for="reconcile-{account.id}">
              Balance shown by your bank today
            </label>
            <input
              id="reconcile-{account.id}"
              type="number"
              step="0.01"
              bind:value={reconcileDraft}
              onkeydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  handleReconcile(account);
                } else if (e.key === "Escape") {
                  cancelReconcile();
                }
              }}
            />
            <button type="button" onclick={() => handleReconcile(account)}>
              Apply
            </button>
            <button type="button" class="secondary" onclick={cancelReconcile}>
              Cancel
            </button>
            <p class="hint">
              The difference is posted as a single adjustment, so this account
              matches what you can actually see. It doesn't count as spending.
            </p>
          </div>
        {/if}
        <div class="patterns">
          <span class="patterns-label" title="Matched against an imported row's source text to decide which account it belongs to">Belongs to this account</span>
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
        <div class="patterns">
          <span
            class="patterns-label"
            title="An imported row matching one of these is money you sent to this account — it's mirrored here automatically, and left out of spending totals"
            >Transfers into this account</span
          >
          {#each rulesFor(account.id) as rule (rule.id)}
            <span class="chip">
              {rule.pattern}
              <button
                type="button"
                onclick={() => handleRemoveTransferRule(rule)}
                aria-label="Remove transfer rule">×</button
              >
            </span>
          {/each}
          <input
            class="pattern-input"
            placeholder="Add transfer pattern…"
            value={newTransferPatternDrafts[account.id] ?? ""}
            oninput={(e) =>
              (newTransferPatternDrafts[account.id] = e.currentTarget.value)}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                handleAddTransferRule(account);
              }
            }}
          />
          {#if rulesFor(account.id).length > 0}
            <button
              type="button"
              class="secondary"
              onclick={() => startApplyTransferRules(account)}
            >
              Apply to past transactions
            </button>
          {/if}
        </div>
        {#if applyingRulesAccountId === account.id}
          <div class="reconcile">
            <p class="hint">
              Rescans every transaction already in the ledger against this
              account's transfer patterns above, converting any match into a
              transfer pair — the same thing a new import would have done, for
              rows imported before the pattern existed.
            </p>
            <button
              type="button"
              onclick={() => handleApplyTransferRules(account)}
            >
              Apply
            </button>
            <button
              type="button"
              class="secondary"
              onclick={cancelApplyTransferRules}
            >
              Cancel
            </button>
          </div>
        {/if}
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

  .patterns-label {
    font-size: 0.8rem;
    opacity: 0.7;
    min-width: 11rem;
  }

  .reconcile {
    margin-top: 0.6rem;
    padding: 0.75rem;
    border-radius: 8px;
    background-color: var(--color-shade-3);
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .reconcile label {
    font-size: 0.85rem;
    opacity: 0.85;
  }

  .reconcile input {
    width: 8rem;
    background-color: var(--color-shade-2);
  }

  .reconcile .hint {
    flex-basis: 100%;
    margin: 0;
    font-size: 0.8rem;
    opacity: 0.7;
  }

  button.secondary {
    background-color: transparent;
    color: inherit;
    border: 1px solid var(--color-shade-3);
  }
</style>
