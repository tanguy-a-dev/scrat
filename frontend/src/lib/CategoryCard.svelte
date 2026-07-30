<script lang="ts">
  import type { CategoryDto } from "./api";
  import DeleteButton from "./DeleteButton.svelte";
  import { Plus, Lock, Unlock } from "@lucide/svelte";

  let {
    category,
    subcategories,
    onRename,
    onDelete,
    onAddChild,
    onSetDefault,
  }: {
    category: CategoryDto;
    /** Direct children only — the hierarchy is never deeper than two levels. */
    subcategories: CategoryDto[];
    onRename: (id: string, name: string) => void;
    onDelete: (category: CategoryDto) => void;
    onAddChild: (parentId: string, name: string) => void;
    onSetDefault: (id: string) => void;
  } = $props();

  let addingChild = $state(false);
  let childName = $state("");

  /* Live text per input, so a pill-shaped subcategory input can size itself to
     its content while being typed in (the DOM value stays uncontrolled — renames
     only commit on change, exactly as before). */
  let drafts = $state<Record<string, string>>({});

  function draftFor(c: CategoryDto): string {
    return drafts[c.id] ?? c.name;
  }

  function submitChild(event: Event) {
    event.preventDefault();
    if (!childName.trim()) return;
    onAddChild(category.id, childName.trim());
    childName = "";
    addingChild = false;
  }

  function autofocus(node: HTMLInputElement) {
    node.focus();
  }
</script>

<section class="card" class:is-default={category.is_default}>
  <header>
    <input
      class="name"
      value={category.name}
      aria-label="Category name"
      onchange={(e) => onRename(category.id, e.currentTarget.value)}
    />
    {#if subcategories.length > 0}
      <span class="count" title="{subcategories.length} subcategories">
        {subcategories.length}
      </span>
    {/if}
    {#if category.is_default}
      <span
        class="icon-button"
        aria-label="Default category"
        title="Default category"
      >
        <Lock size={16} />
      </span>
    {:else}
      <button
        type="button"
        class="icon-button dim"
        aria-label="Set as default"
        title="Set as default"
        onclick={() => onSetDefault(category.id)}
      >
        <Unlock size={16} />
      </button>
    {/if}
    <span class="dim">
      <DeleteButton
        label="Delete category"
        onConfirm={() => onDelete(category)}
      />
    </span>
  </header>

  <div class="subs">
    {#each subcategories as child (child.id)}
      <span class="chip" class:is-default={child.is_default}>
        <input
          class="chip-name"
          value={draftFor(child)}
          size={Math.max(draftFor(child).length + 1, 4)}
          aria-label="Subcategory name"
          oninput={(e) => (drafts[child.id] = e.currentTarget.value)}
          onchange={(e) => onRename(child.id, e.currentTarget.value)}
        />
        {#if child.is_default}
          <span
            class="chip-action"
            aria-label="Default category"
            title="Default category"
          >
            <Lock size={13} />
          </span>
        {:else}
          <button
            type="button"
            class="chip-action"
            aria-label="Set as default"
            title="Set as default"
            onclick={() => onSetDefault(child.id)}
          >
            <Unlock size={13} />
          </button>
        {/if}
        <DeleteButton
          label="Delete subcategory"
          compact
          onConfirm={() => onDelete(child)}
        />
      </span>
    {/each}

    {#if addingChild}
      <form class="chip add-form" onsubmit={submitChild}>
        <input
          class="chip-name"
          placeholder="Subcategory"
          size={Math.max(childName.length + 2, 12)}
          bind:value={childName}
          use:autofocus
          onkeydown={(e) => {
            if (e.key === "Escape") addingChild = false;
          }}
          onblur={() => {
            if (!childName.trim()) addingChild = false;
          }}
        />
        <button type="submit" class="chip-action" aria-label="Add subcategory">
          <Plus size={13} />
        </button>
      </form>
    {:else}
      <button
        type="button"
        class="chip add-chip"
        onclick={() => (addingChild = true)}
      >
        <Plus size={13} />
        Subcategory
      </button>
    {/if}
  </div>
</section>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 0.85rem 0.9rem;
    border-radius: 12px;
    border: 1px solid transparent;
    background-color: var(--color-shade-2);
  }

  .card.is-default {
    border-color: color-mix(in srgb, var(--color-accent) 45%, transparent);
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  /* Names stay directly editable, but read as text until you reach for them —
     a grid of boxed inputs was the main source of visual noise here. */
  .name {
    flex: 1;
    min-width: 0;
    border: 1px solid transparent;
    border-radius: 6px;
    background-color: transparent;
    color: inherit;
    padding: 0.25rem 0.4rem;
    font-family: inherit;
    font-size: 1rem;
    font-weight: 600;
  }

  .name:hover {
    background-color: var(--color-shade-3);
  }

  .name:focus {
    outline: none;
    background-color: var(--color-shade-1);
    border-color: var(--color-accent);
  }

  .count {
    flex-shrink: 0;
    min-width: 1.5rem;
    text-align: center;
    padding: 0.1rem 0.4rem;
    border-radius: 999px;
    background-color: var(--color-shade-3);
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    opacity: 0.85;
  }

  /* Secondary actions recede until the card is being worked on, so a grid of
     cards reads as content rather than as a wall of buttons. Space is always
     reserved, so nothing reflows on hover. */
  .dim {
    display: inline-flex;
    opacity: 0.3;
    transition: opacity 0.12s ease;
  }

  .card:hover .dim,
  .dim:focus-within {
    opacity: 1;
  }

  .subs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem;
    padding-top: 0.6rem;
    border-top: 1px solid var(--color-shade-3);
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.1rem;
    padding: 0.1rem 0.2rem 0.1rem 0.1rem;
    border: 1px solid transparent;
    border-radius: 999px;
    background-color: var(--color-shade-3);
    font-size: 0.85rem;
  }

  .chip.is-default {
    border-color: color-mix(in srgb, var(--color-accent) 45%, transparent);
  }

  .chip-name {
    width: auto;
    min-width: 0;
    border: 1px solid transparent;
    border-radius: 999px;
    background-color: transparent;
    color: inherit;
    padding: 0.15rem 0.5rem;
    font-family: inherit;
    font-size: 0.85rem;
  }

  .chip-name:focus {
    outline: none;
    background-color: var(--color-shade-1);
    border-color: var(--color-accent);
  }

  .chip-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.35rem;
    height: 1.35rem;
    flex-shrink: 0;
    padding: 0;
    border: none;
    border-radius: 50%;
    background-color: transparent;
    color: inherit;
    opacity: 0.3;
    cursor: pointer;
  }

  .chip-action:hover {
    opacity: 1;
    background-color: var(--color-shade-4);
  }

  .chip.is-default .chip-action {
    color: var(--color-accent);
    opacity: 1;
    cursor: default;
  }

  .add-chip {
    gap: 0.25rem;
    padding: 0.25rem 0.6rem 0.25rem 0.45rem;
    border: 1px dashed var(--color-shade-4);
    background-color: transparent;
    color: inherit;
    font-family: inherit;
    opacity: 0.6;
    cursor: pointer;
  }

  .add-chip:hover {
    opacity: 1;
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  .add-form {
    background-color: var(--color-shade-3);
  }
</style>
