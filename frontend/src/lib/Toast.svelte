<script lang="ts">
  import { fly } from "svelte/transition";
  import { CircleCheck, CircleX, Info, X } from "@lucide/svelte";
  import { toast } from "./toasts.svelte";
  import { t } from "./i18n.svelte";
</script>

<div class="toast-host">
  {#each toast.items as item (item.id)}
    <div class="toast {item.kind}" transition:fly={{ y: 16, duration: 200 }}>
      <span class="icon">
        {#if item.kind === "success"}
          <CircleCheck size={18} />
        {:else if item.kind === "error"}
          <CircleX size={18} />
        {:else}
          <Info size={18} />
        {/if}
      </span>
      <span class="message">{item.message}</span>
      <button
        type="button"
        class="icon-button"
        aria-label={t("component.dismiss")}
        onclick={() => toast.dismiss(item.id)}
      >
        <X size={14} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toast-host {
    position: fixed;
    bottom: 1.25rem;
    right: 1.25rem;
    z-index: 2000;
    display: flex;
    flex-direction: column-reverse;
    gap: 0.5rem;
    max-width: 22rem;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.7rem 0.8rem;
    border-radius: 8px;
    background-color: var(--color-shade-2);
    border: 1px solid var(--color-shade-3);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
    font-size: 0.85rem;
  }

  .icon {
    display: inline-flex;
    flex-shrink: 0;
  }

  .toast.success .icon {
    color: var(--color-success);
  }

  .toast.error .icon {
    color: var(--color-danger);
  }

  .toast.info .icon {
    color: var(--color-accent);
  }

  .message {
    flex: 1;
    line-height: 1.3;
  }
</style>
