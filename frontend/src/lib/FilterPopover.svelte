<script lang="ts">
  import type { Snippet } from "svelte";
  import { Search } from "@lucide/svelte";

  let {
    active = false,
    ariaLabel,
    children,
  }: {
    active?: boolean;
    ariaLabel: string;
    children: Snippet;
  } = $props();

  let open = $state(false);
  let containerEl: HTMLDivElement | undefined = $state();

  function toggle() {
    open = !open;
  }

  function handleWindowClick(event: MouseEvent) {
    if (open && containerEl && !containerEl.contains(event.target as Node)) {
      open = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      open = false;
    }
  }
</script>

<svelte:window onclick={handleWindowClick} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="filter-popover" bind:this={containerEl} onkeydown={handleKeydown}>
  <button
    type="button"
    class="trigger"
    class:active
    onclick={toggle}
    aria-label={ariaLabel}
  >
    <Search size={14} />
  </button>
  {#if open}
    <div class="dropdown">
      {@render children()}
    </div>
  {/if}
</div>

<style>
  .filter-popover {
    position: relative;
    display: inline-flex;
  }

  .trigger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    border: 1px solid var(--color-shade-3);
    background-color: var(--color-shade-2);
    color: inherit;
    padding: 0.3rem;
    cursor: pointer;
  }

  .trigger.active {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
    border-color: var(--color-accent);
  }

  .dropdown {
    position: absolute;
    top: calc(100% + 0.25rem);
    left: 0;
    z-index: 100;
    width: max(100%, 14rem);
    background: var(--color-shade-2);
    border: 1px solid var(--color-shade-3);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    overflow: hidden;
  }

  .dropdown :global(input) {
    width: 100%;
    box-sizing: border-box;
    border: none;
    background: transparent;
    color: inherit;
    padding: 0.5rem 0.6rem;
    font-size: 0.85rem;
    font-family: inherit;
  }

  .dropdown :global(input:focus) {
    outline: none;
  }
</style>
