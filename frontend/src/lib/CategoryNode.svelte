<script lang="ts">
  import type { CategoryDto } from "./api";
  import CategoryNode from "./CategoryNode.svelte";

  let {
    category,
    all,
    onRename,
    onDelete,
    onAddChild,
    depth = 0,
  }: {
    category: CategoryDto;
    all: CategoryDto[];
    onRename: (id: string, name: string) => void;
    onDelete: (category: CategoryDto) => void;
    onAddChild: (parentId: string, name: string) => void;
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
    <button type="button" onclick={() => (addingChild = !addingChild)}>
      + sub
    </button>
    <button type="button" class="danger" onclick={() => onDelete(category)}>
      Delete
    </button>
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
    border: 1px solid rgba(0, 0, 0, 0.15);
    padding: 0.35rem 0.6rem;
    font-family: inherit;
    font-size: 0.95rem;
    min-width: 12rem;
  }

  button {
    border-radius: 6px;
    border: none;
    padding: 0.35rem 0.6rem;
    font-size: 0.85rem;
    cursor: pointer;
    background-color: #396cd8;
    color: white;
  }

  button.danger {
    background-color: #b3261e;
  }

  .add-child {
    display: flex;
    gap: 0.5rem;
    padding-bottom: 0.5rem;
  }

  .add-child input {
    border-radius: 6px;
    border: 1px solid rgba(0, 0, 0, 0.15);
    padding: 0.35rem 0.6rem;
    font-family: inherit;
  }

  @media (prefers-color-scheme: dark) {
    .name,
    .add-child input {
      background-color: rgba(255, 255, 255, 0.06);
      border-color: rgba(255, 255, 255, 0.15);
      color: inherit;
    }
  }
</style>
