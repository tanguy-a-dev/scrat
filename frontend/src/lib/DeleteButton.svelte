<script lang="ts">
  import { Trash2, Check, X } from "@lucide/svelte";
  import { t } from "$lib/i18n.svelte";

  let {
    label,
    onConfirm,
    compact = false,
  }: {
    /** aria-label/title for the trigger, e.g. "Delete transaction". */
    label: string;
    onConfirm: () => void;
    /** Smaller, unfilled variant that fits inside a chip or pill. */
    compact?: boolean;
  } = $props();

  let confirming = $state(false);
  let rootEl: HTMLElement | undefined = $state();

  function cancel() {
    confirming = false;
  }

  function confirmDelete() {
    confirming = false;
    onConfirm();
  }

  $effect(() => {
    if (!confirming) return;
    function handleClickOutside(event: MouseEvent) {
      if (rootEl && !rootEl.contains(event.target as Node)) {
        confirming = false;
      }
    }
    function handleKeydown(event: KeyboardEvent) {
      if (event.key === "Escape") cancel();
    }
    window.addEventListener("mousedown", handleClickOutside);
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("mousedown", handleClickOutside);
      window.removeEventListener("keydown", handleKeydown);
    };
  });
</script>

<span class="delete-button" class:compact bind:this={rootEl}>
  {#if confirming}
    <span class="confirm-popover">
      <span class="confirm-text">{t("component.deleteConfirm")}</span>
      <button
        type="button"
        class="icon-button danger"
        aria-label={t("component.confirmDelete")}
        onclick={confirmDelete}
      >
        <Check size={compact ? 13 : 16} />
      </button>
      <button
        type="button"
        class="icon-button"
        aria-label={t("common.cancel")}
        onclick={cancel}
      >
        <X size={compact ? 13 : 16} />
      </button>
    </span>
  {:else}
    <button
      type="button"
      class="icon-button danger"
      aria-label={label}
      title={label}
      onclick={() => (confirming = true)}
    >
      <Trash2 size={compact ? 13 : 16} />
    </button>
  {/if}
</span>

<style>
  .delete-button {
    display: inline-flex;
    position: relative;
  }

  /* The confirm step is a transient overlay, not a wider control. In flow it
     was ~120px wider than the trash button it replaces, and in a table cell
     that widened the column, the table, and the page — clicking delete on the
     right-hand (Income) list pushed the whole layout past the right edge of
     the window and forced a horizontal scroll to reach the buttons being
     asked about. Pinning the footprint to the trigger's own 2rem and letting
     the popover grow leftward out of flow means opening it moves nothing.
     Compact (the bulk-actions pill) keeps the in-flow version: it lives in a
     flex header with room to grow, and overlaying there would cover the
     "N selected" count the confirmation is about. */
  .delete-button:not(.compact) {
    width: 2rem;
    height: 2rem;
  }

  .confirm-popover {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    background-color: var(--color-shade-3);
    border-radius: 999px;
    padding: 0.15rem 0.15rem 0.15rem 0.7rem;
  }

  .delete-button:not(.compact) .confirm-popover {
    position: absolute;
    top: 50%;
    right: 0;
    transform: translateY(-50%);
    /* Above the row's own content it slides over, and above the category
       SearchSelect next door, which sits at z-index 1. */
    z-index: 5;
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.45);
  }

  .confirm-text {
    font-size: 0.8rem;
    white-space: nowrap;
  }

  /* Compact: same confirm flow, sized to sit inline in a pill. The trigger
     drops the filled circle so a row of chips doesn't read as a row of
     buttons — colour alone carries the "destructive" signal until hover. */
  .compact .icon-button {
    width: 1.35rem;
    height: 1.35rem;
    background-color: transparent;
    color: var(--color-danger);
    opacity: 0.45;
  }

  .compact .confirm-popover .icon-button,
  .compact .icon-button:hover {
    opacity: 1;
  }

  .compact .icon-button:hover {
    background-color: color-mix(in srgb, var(--color-danger) 30%, transparent);
    color: var(--color-danger-strong);
  }

  .compact .confirm-popover {
    background-color: var(--color-shade-4);
    padding: 0.1rem 0.1rem 0.1rem 0.5rem;
  }

  .compact .confirm-text {
    font-size: 0.7rem;
  }
</style>
