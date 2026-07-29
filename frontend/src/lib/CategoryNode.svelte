<script lang="ts">
  import type { CategoryDto } from "./api";
  import CategoryNode from "./CategoryNode.svelte";
  import DeleteButton from "./DeleteButton.svelte";
  import { Plus, Lock, Unlock } from "@lucide/svelte";

  let {
    category,
    all,
    onRename,
    onDelete,
    onAddChild,
    onSetDefault,
    depth = 0,
  }: {
    category: CategoryDto;
    all: CategoryDto[];
    onRename: (id: string, name: string) => void;
    onDelete: (category: CategoryDto) => void;
    onAddChild: (parentId: string, name: string) => void;
    onSetDefault: (id: string) => void;
    depth?: number;
  } = $props();

  let children = $derived(all.filter((c) => c.parent_id === category.id));
  let addingChild = $state(false);
  let childName = $state("");

  function submitChild(event: Event) {
    event.preventDefault();
    if (!childName.trim()) return;
    onAddChild(category.id, childName.trim());
    childName = "";
    addingChild = false;
  }
</script>

<li>
  <div class="node" style={`padding-left: ${depth * 1.25}rem`}>
    <input
      class="name"
      value={category.name}
      onchange={(e) => onRename(category.id, e.currentTarget.value)}
    />
    {#if category.is_default}
      <span class="icon-button" aria-label="Default category" title="Default category">
        <Lock size={16} />
      </span>
    {:else}
      <button
        type="button"
        class="icon-button"
        aria-label="Set as default"
        title="Set as default"
        onclick={() => onSetDefault(category.id)}
      >
        <Unlock size={16} />
      </button>
    {/if}
    {#if depth === 0}
      <button
        type="button"
        class="icon-button"
        aria-label="Add subcategory"
        title="Add subcategory"
        onclick={() => (addingChild = !addingChild)}
      >
        <Plus size={16} />
      </button>
    {/if}
    <DeleteButton label="Delete category" onConfirm={() => onDelete(category)} />
  </div>
  {#if addingChild}
    <form
      class="add-child"
      style={`padding-left: ${(depth + 1) * 1.25}rem`}
      onsubmit={submitChild}
    >
      <input placeholder="Subcategory name" bind:value={childName} />
      <button type="submit">Add</button>
    </form>
  {/if}
  {#if children.length > 0}
    <ul>
      {#each children as child (child.id)}
        <CategoryNode
          category={child}
          {all}
          {onRename}
          {onDelete}
          {onAddChild}
          {onSetDefault}
          depth={depth + 1}
        />
      {/each}
    </ul>
  {/if}
</li>

<style>
  li {
    list-style: none;
  }

  ul {
    margin: 0;
    padding: 0;
  }

  .node {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding-top: 0.35rem;
    padding-bottom: 0.35rem;
  }

  .name {
    border-radius: 6px;
    border: 1px solid var(--color-shade-3);
    background-color: var(--color-shade-2);
    color: inherit;
    padding: 0.35rem 0.6rem;
    font-family: inherit;
    font-size: 0.95rem;
    min-width: 12rem;
  }

  button:not(.icon-button) {
    border-radius: 6px;
    border: none;
    padding: 0.35rem 0.6rem;
    font-size: 0.85rem;
    cursor: pointer;
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
  }

  .add-child {
    display: flex;
    gap: 0.5rem;
    padding-bottom: 0.5rem;
  }

  .add-child input {
    border-radius: 6px;
    border: 1px solid var(--color-shade-3);
    background-color: var(--color-shade-2);
    color: inherit;
    padding: 0.35rem 0.6rem;
    font-family: inherit;
  }
</style>
