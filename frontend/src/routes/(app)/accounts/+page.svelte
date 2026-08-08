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
  import { describeError, t, tp } from "$lib/i18n.svelte";

  let accounts = $state<AccountDto[]>([]);
  let transferRules = $state<TransferRuleDto[]>([]);
  let loading = $state(true);
  let error = $state("");

  let newName = $state("");
  let newPatternDrafts = $state<Record<string, string>>({});
  let newTransferPatternDrafts = $state<Record<string, string>>({});
  /** Which account has its adjustment input open, and what's typed in it.
   * Only one at a time — posting an adjustment is a deliberate act against a
   * number read off another app, not something to have half-open
   * everywhere. */
  let reconcilingAccountId = $state<string | null>(null);
  let reconcileDraft = $state("");
  /** Which account's "apply to past transactions" confirm is open. Only one
   * at a time, same reasoning as `reconcilingAccountId`. */
  let applyingRulesAccountId = $state<string | null>(null);
  /** Which account has its starting-point input open, and what's typed in
   * it. Same one-at-a-time rule as the adjustment panel — and deliberately
   * separate state, because the two ask for the same number and mean
   * different things. */
  let anchoringAccountId = $state<string | null>(null);
  let anchorDraft = $state("");

  const anchoringAccount = $derived(
    accounts.find((a) => a.id === anchoringAccountId) ?? null,
  );
  const reconcilingAccount = $derived(
    accounts.find((a) => a.id === reconcilingAccountId) ?? null,
  );

  /** SUM(transactions) for an account. Derived rather than fetched: a
   * balance *is* the starting point plus the ledger, so subtracting the
   * anchor back out is exact, not an estimate. */
  function ledgerSum(account: AccountDto): number {
    return account.balance_minor_units - account.opening_balance_minor_units;
  }

  /** What the starting point would work out to for what's typed right now —
   * the same `observed - ledger sum` the backend computes, shown before the
   * user commits to it. Null while the input isn't a number.
   *
   * Showing the result is what makes the two panels tellable apart without
   * understanding anchors versus adjustments: you pick the one whose outcome
   * line says what you meant. It's also what makes editing an existing
   * starting point safe — otherwise the number being replaced is invisible. */
  const anchorPreview = $derived.by(() => {
    if (!anchoringAccount) return null;
    const observed = parseToMinorUnits(anchorDraft);
    if (observed === null) return null;
    return observed - ledgerSum(anchoringAccount);
  });

  /** The adjustment that would be posted: what the bank says minus what the
   * app currently believes. Null while the input isn't a number, and zero is
   * a meaningful value — it's the "nothing to adjust" case, worth showing
   * rather than hiding, so Apply isn't a mystery no-op. */
  const adjustmentPreview = $derived.by(() => {
    if (!reconcilingAccount) return null;
    const observed = parseToMinorUnits(reconcileDraft);
    if (observed === null) return null;
    return observed - reconcilingAccount.balance_minor_units;
  });

  onMount(load);

  /** `silent` skips the `loading` flag, so the account list isn't unmounted
   * and remounted on every refresh after a mutation — that used to destroy
   * the focused input (and jump the scroll position to the top) whenever an
   * account further down the list was being edited. Only the very first
   * load, before there's anything on screen yet, needs the loading state. */
  async function load(silent = false) {
    if (!silent) loading = true;
    error = "";
    try {
      const [a, r] = await Promise.all([
        api.listAccounts(),
        api.listTransferRules(),
      ]);
      accounts = a;
      transferRules = r;
    } catch (e) {
      error = describeError(e);
    } finally {
      if (!silent) loading = false;
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
      await load(true);
    } catch (e) {
      toast.error(describeError(e));
    }
  }

  async function withDeleteConfirmation(
    action: () => Promise<unknown>,
    successMessage: string,
  ) {
    try {
      await action();
      await load(true);
      toast.success(successMessage);
    } catch (e) {
      toast.error(describeError(e));
    }
  }

  function handleCreate(event: Event) {
    event.preventDefault();
    // No balance asked for here on purpose: whatever the user typed would be
    // wrong the moment they imported history, since that moves the starting
    // point back before the first imported row. It's established afterwards,
    // from a balance they can actually read off their bank.
    withErrorHandling(async () => {
      await api.createAccount(newName);
      newName = "";
    });
  }

  function handleRename(account: AccountDto, name: string) {
    if (name.trim() === account.name) return;
    withErrorHandling(() => api.renameAccount(account.id, name));
  }

  function startAnchor(account: AccountDto) {
    // The two panels ask for the same number and do opposite things with it,
    // so they must never be on screen together — side by side they'd read as
    // a choice between two identical forms.
    cancelReconcile();
    anchoringAccountId = account.id;
    // Pre-filled with the balance on screen rather than left blank: if the
    // account really did start at zero, that number is already correct and
    // submitting it unchanged is the right answer. It's also what makes
    // opening the panel to *edit* an anchor harmless — unchanged input means
    // unchanged anchor, so looking costs nothing.
    anchorDraft = formatMinorUnits(account.balance_minor_units);
  }

  function cancelAnchor() {
    anchoringAccountId = null;
    anchorDraft = "";
  }

  async function handleAnchor(account: AccountDto) {
    const minorUnits = parseToMinorUnits(anchorDraft);
    if (minorUnits === null) {
      toast.error(t("accounts.balanceNotANumber"));
      return;
    }
    const wasSet = account.is_opening_balance_set;
    try {
      await api.establishOpeningBalance(account.id, minorUnits);
      cancelAnchor();
      await load(true);
      toast.success(
        wasSet
          ? t("accounts.startingPointUpdated", { name: account.name })
          : t("accounts.startingPointSet", { name: account.name }),
      );
    } catch (e) {
      toast.error(describeError(e));
    }
  }

  function handleAddPattern(account: AccountDto) {
    const pattern = (newPatternDrafts[account.id] ?? "").trim();
    if (!pattern) return;
    withErrorHandling(async () => {
      await api.addDescriptionPattern(account.id, pattern);
      newPatternDrafts[account.id] = "";
    });
  }

  function handleRemovePattern(account: AccountDto, pattern: string) {
    withErrorHandling(() => api.removeDescriptionPattern(account.id, pattern));
  }

  function handleDelete(account: AccountDto) {
    withDeleteConfirmation(
      () => api.deleteAccount(account.id),
      t("accounts.deleted", { name: account.name }),
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
      await load(true);
      toast.success(
        summary.converted > 0
          ? tp("accounts.converted", summary.converted)
          : t("accounts.noMatchesFound"),
      );
    } catch (e) {
      toast.error(describeError(e));
    }
  }

  function startReconcile(account: AccountDto) {
    cancelAnchor(); // see startAnchor
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
      toast.error(t("accounts.balanceNotANumber"));
      return;
    }
    try {
      const adjustment = await api.reconcileAccount(
        account.id,
        minorUnits,
        todayIsoDate(),
      );
      cancelReconcile();
      await load(true);
      toast.success(
        adjustment
          ? t("accounts.adjustedBy", {
              amount: formatCurrency(adjustment.amount_minor_units, account.currency),
            })
          : t("accounts.alreadyMatched", { name: account.name }),
      );
    } catch (e) {
      toast.error(describeError(e));
    }
  }
</script>

<h1>{t("accounts.title")}</h1>

{#if error}
  <p class="error">{error}</p>
{/if}

<form class="create-form" onsubmit={handleCreate}>
  <input placeholder={t("accounts.namePlaceholder")} bind:value={newName} required />
  <button type="submit">{t("accounts.addAccount")}</button>
</form>

{#if loading}
  <p>{t("common.loading")}</p>
{:else if accounts.length === 0}
  <p class="empty">{t("accounts.empty")}</p>
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
          <span
            class="computed"
            class:provisional={!account.is_opening_balance_set &&
              account.has_transactions}
            >{t("accounts.balanceLabel", {
              amount: formatCurrency(account.balance_minor_units, account.currency),
            })}</span
          >
          {#if account.is_default}
            <span class="default-badge">{t("accounts.default")}</span>
          {:else}
            <button type="button" onclick={() => handleSetDefault(account)}>
              {t("accounts.setAsDefault")}
            </button>
          {/if}
          <!-- Shown whether or not the anchor is set: a mistyped starting
               point is otherwise permanent, since nothing else can move it. -->
          <button type="button" onclick={() => startAnchor(account)}>
            {account.is_opening_balance_set
              ? t("accounts.editStartingPoint")
              : t("accounts.setStartingPoint")}
          </button>
          <button type="button" onclick={() => startReconcile(account)}>
            {t("accounts.addAdjustment")}
          </button>
          <DeleteButton
            label={t("accounts.deleteAccount")}
            onConfirm={() => handleDelete(account)}
          />
        </div>
        <!-- Only a problem once there are transactions to anchor: an empty
             account is at zero either way, so flagging it would be noise. -->
        {#if !account.is_opening_balance_set && account.has_transactions && anchoringAccountId !== account.id}
          <p class="unanchored">{t("accounts.unanchored")}</p>
        {/if}
        {#if anchoringAccountId === account.id}
          <div class="reconcile">
            <p class="panel-title">
              {account.is_opening_balance_set
                ? t("accounts.editStartingPoint")
                : t("accounts.setStartingPoint")}
              <span>{t("accounts.noLedgerEntry")}</span>
            </p>
            <label for="anchor-{account.id}">
              {t("accounts.bankBalanceToday")}
            </label>
            <input
              id="anchor-{account.id}"
              type="number"
              step="0.01"
              bind:value={anchorDraft}
              onkeydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  handleAnchor(account);
                } else if (e.key === "Escape") {
                  cancelAnchor();
                }
              }}
            />
            <button type="button" onclick={() => handleAnchor(account)}>
              {t("range.apply")}
            </button>
            <button type="button" class="secondary" onclick={cancelAnchor}>
              {t("common.cancel")}
            </button>
            <!-- The arithmetic, shown rather than explained: the user can
                 check the outcome against what they meant without having to
                 know the formula, or that there is one. -->
            {#if anchorPreview !== null}
              <dl class="preview">
                <div>
                  <dt>{t("accounts.transactionsOnRecord")}</dt>
                  <dd>{formatCurrency(ledgerSum(account), account.currency)}</dd>
                </div>
                {#if account.is_opening_balance_set}
                  <div>
                    <dt>{t("accounts.startingPointNow")}</dt>
                    <dd>
                      {formatCurrency(
                        account.opening_balance_minor_units,
                        account.currency,
                      )}
                    </dd>
                  </div>
                {/if}
                <div class="result">
                  <dt>{t("accounts.startingPointBecomes")}</dt>
                  <dd>{formatCurrency(anchorPreview, account.currency)}</dd>
                </div>
              </dl>
            {/if}
            <p class="hint">
              {t("accounts.anchorHint")}
              {#if account.is_opening_balance_set}
                {t("accounts.anchorHintReplaces")}
              {/if}
            </p>
          </div>
        {/if}
        {#if reconcilingAccountId === account.id}
          <div class="reconcile">
            <p class="panel-title">
              {t("accounts.addAdjustment")}
              <span>{t("accounts.oneEntryDatedToday")}</span>
            </p>
            <label for="reconcile-{account.id}">
              {t("accounts.bankBalanceToday")}
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
              {t("range.apply")}
            </button>
            <button type="button" class="secondary" onclick={cancelReconcile}>
              {t("common.cancel")}
            </button>
            <!-- Same reasoning as the starting-point preview: the panels are
                 told apart by their outcome, not by their input. It also
                 makes the zero case legible, so Apply is never a silent
                 no-op. -->
            {#if adjustmentPreview !== null}
              <dl class="preview">
                <div>
                  <dt>{t("accounts.appCurrentlyShows")}</dt>
                  <dd>
                    {formatCurrency(
                      account.balance_minor_units,
                      account.currency,
                    )}
                  </dd>
                </div>
                <div class="result">
                  <dt>{t("accounts.adjustmentPosted")}</dt>
                  <dd>
                    {#if adjustmentPreview === 0}
                      {t("accounts.adjustmentNone")}
                    {:else}
                      {adjustmentPreview > 0
                        ? "+"
                        : ""}{formatCurrency(
                        adjustmentPreview,
                        account.currency,
                      )}
                    {/if}
                  </dd>
                </div>
              </dl>
            {/if}
            <p class="hint">{t("accounts.reconcileHint")}</p>
          </div>
        {/if}
        <div class="patterns">
          <span class="patterns-label" title={t("accounts.belongsTitle")}>{t("accounts.belongsToThisAccount")}</span>
          {#each account.description_patterns as pattern (pattern)}
            <span class="chip">
              {pattern}
              <button
                type="button"
                onclick={() => handleRemovePattern(account, pattern)}
                aria-label={t("accounts.removePattern")}>×</button
              >
            </span>
          {/each}
          <input
            class="pattern-input"
            placeholder={t("accounts.addPatternPlaceholder")}
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
            title={t("accounts.transfersIntoTitle")}
            >{t("accounts.transfersInto")}</span
          >
          {#each rulesFor(account.id) as rule (rule.id)}
            <span class="chip">
              {rule.pattern}
              <button
                type="button"
                onclick={() => handleRemoveTransferRule(rule)}
                aria-label={t("accounts.removeTransferRule")}>×</button
              >
            </span>
          {/each}
          <input
            class="pattern-input"
            placeholder={t("accounts.addTransferPatternPlaceholder")}
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
              {t("accounts.applyToPast")}
            </button>
          {/if}
        </div>
        {#if applyingRulesAccountId === account.id}
          <div class="reconcile">
            <p class="hint">{t("accounts.applyRulesHint")}</p>
            <button
              type="button"
              onclick={() => handleApplyTransferRules(account)}
            >
              {t("range.apply")}
            </button>
            <button
              type="button"
              class="secondary"
              onclick={cancelApplyTransferRules}
            >
              {t("common.cancel")}
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

  .computed {
    opacity: 0.8;
    font-size: 0.9rem;
  }

  /* A balance the app can't vouch for, because the account's starting point
     is still unknown. Dotted rather than coloured — it's provisional, not
     an error. */
  .computed.provisional {
    text-decoration: underline dotted;
    text-underline-offset: 0.25em;
  }

  .unanchored {
    margin: 0.6rem 0 0;
    font-size: 0.8rem;
    opacity: 0.75;
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

  /* Both panels ask the same question ("what does your bank say?") and do
     opposite things with the answer, so the title — not the field label —
     is what tells them apart. */
  .reconcile .panel-title {
    flex-basis: 100%;
    margin: 0;
    font-size: 0.9rem;
    font-weight: 600;
  }

  .reconcile .panel-title span {
    font-weight: 400;
    opacity: 0.7;
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

  /* The working-out behind whichever panel is open. Deliberately laid out
     like a till receipt — inputs above, outcome on the last line — so the
     line that matters is the one the eye lands on. */
  .preview {
    flex-basis: 100%;
    margin: 0;
    font-size: 0.8rem;
  }

  /* Width is capped on the rows, not on .preview itself: a max-width there
     shrinks the hypothetical main size below 100%, and the receipt stops
     reliably breaking onto its own line. */
  .preview div {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    max-width: 22rem;
    padding: 0.15rem 0;
  }

  .preview dt {
    opacity: 0.7;
  }

  .preview dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }

  .preview .result {
    border-top: 1px solid var(--color-shade-2);
    margin-top: 0.15rem;
    padding-top: 0.3rem;
    font-weight: 600;
  }

  .preview .result dt {
    opacity: 1;
  }

  button.secondary {
    background-color: transparent;
    color: inherit;
    border: 1px solid var(--color-shade-3);
  }
</style>
