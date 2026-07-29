<script lang="ts">
  import { Trash2, Check, X } from "@lucide/svelte";

  let {
    label,
    onConfirm,
  }: {
    /** aria-label/title for the trigger, e.g. "Delete transaction". */
    label: string;
    onConfirm: () => void;
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

<span class="delete-button" bind:this={rootEl}>
  {#if confirming}
    <span class="confirm-popover">
      <span class="confirm-text">Delete?</span>
      <button
        type="button"
        class="icon-button danger"
        aria-label="Confirm delete"
        onclick={confirmDelete}
      >
        <Check size={16} />
      </button>
      <button
        type="button"
        class="icon-button"
        aria-label="Cancel"
        onclick={cancel}
      >
        <X size={16} />
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
      <Trash2 size={16} />
    </button>
  {/if}
</span>

<style>
  .delete-button {
    display: inline-flex;
  }

  .confirm-popover {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    background-color: var(--color-shade-3);
    border-radius: 999px;
    padding: 0.15rem 0.15rem 0.15rem 0.7rem;
  }

  .confirm-text {
    font-size: 0.8rem;
    white-space: nowrap;
  }
</style>
