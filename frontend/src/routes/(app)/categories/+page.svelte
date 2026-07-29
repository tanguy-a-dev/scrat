<script lang="ts">
  import { onMount } from "svelte";
  import { api, type CategoryDto } from "$lib/api";
  import CategoryNode from "$lib/CategoryNode.svelte";
  import { toast } from "$lib/toasts.svelte";

  let categories = $state<CategoryDto[]>([]);
  let loading = $state(true);
  let error = $state("");
  let newRootName = $state("");

  let pendingDelete = $state<{ category: CategoryDto; message: string } | null>(
    null,
  );
  let reassignTarget = $state("");

  onMount(load);

  async function load() {
    loading = true;
    error = "";
    try {
      categories = await api.listCategories();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function withErrorHandling(action: () => Promise<unknown>) {
    try {
      await action();
      await load();
    } catch (e) {
      toast.error(String(e));
    }
  }

  function handleAddRoot(event: Event) {
    event.preventDefault();
    if (!newRootName.trim()) return;
    withErrorHandling(async () => {
      await api.createCategory(newRootName.trim(), null);
      newRootName = "";
    });
  }

  function handleRename(id: string, name: string) {
    if (!name.trim()) return;
    withErrorHandling(() => api.renameCategory(id, name.trim()));
  }

  function handleAddChild(parentId: string, name: string) {
    withErrorHandling(() => api.createCategory(name, parentId));
  }

  function handleSetDefault(id: string) {
    withErrorHandling(() => api.setDefaultCategory(id));
  }

  async function handleDelete(category: CategoryDto) {
    try {
      await api.deleteCategory(category.id, null);
      await load();
      toast.success(`"${category.name}" deleted.`);
    } catch (e) {
      const message = String(e);
      if (message.includes("reassign")) {
        pendingDelete = { category, message };
        reassignTarget = "";
      } else {
        toast.error(message);
      }
    }
  }

  async function confirmReassignDelete() {
    if (!pendingDelete || !reassignTarget) return;
    const { category } = pendingDelete;
    try {
      await api.deleteCategory(category.id, reassignTarget);
      await load();
      toast.success(`"${category.name}" deleted.`);
      pendingDelete = null;
    } catch (e) {
      toast.error(String(e));
    }
  }

  let rootCategories = $derived(categories.filter((c) => c.parent_id === null));
</script>

<h1>Categories</h1>

{#if error}<p class="error">{error}</p>{/if}

<form class="create-form" onsubmit={handleAddRoot}>
  <input placeholder="New category name" bind:value={newRootName} />
  <button type="submit">Add category</button>
</form>

{#if loading}
  <p>Loading…</p>
{:else if rootCategories.length === 0}
  <p class="empty">No categories yet — add one above.</p>
{:else}
  <ul class="tree">
    {#each rootCategories as category (category.id)}
      <CategoryNode
        {category}
        all={categories}
        onRename={handleRename}
        onDelete={handleDelete}
        onAddChild={handleAddChild}
        onSetDefault={handleSetDefault}
      />
    {/each}
  </ul>
{/if}

{#if pendingDelete}
  {@const target = pendingDelete.category}
  {@const targetHasChildren = categories.some(
    (c) => c.parent_id === target.id,
  )}
  <div class="reassign-panel">
    <p>
      "{target.name}" still has transactions. Choose a category to move them
      to before deleting:
    </p>
    <select bind:value={reassignTarget}>
      <option value="" disabled selected>Select a category…</option>
      {#each categories.filter((c) => c.id !== target.id && (!targetHasChildren || c.parent_id === null)) as c (c.id)}
        <option value={c.id}>{c.name}</option>
      {/each}
    </select>
    <button
      type="button"
      onclick={confirmReassignDelete}
      disabled={!reassignTarget}
    >
      Reassign &amp; delete
    </button>
    <button type="button" onclick={() => (pendingDelete = null)}>
      Cancel
    </button>
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

  .create-form {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1.5rem;
  }

  input,
  button {
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

  .create-form button,
  .reassign-panel button {
    cursor: pointer;
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
    border: none;
  }

  .tree {
    margin: 0;
    padding: 0;
  }

  .reassign-panel {
    margin-top: 1.5rem;
    padding: 1rem;
    border-radius: 10px;
    background-color: color-mix(in srgb, var(--color-danger) 15%, transparent);
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    max-width: 28rem;
  }

  .reassign-panel select {
    border-radius: 6px;
    border: 1px solid var(--color-shade-3);
    background-color: var(--color-shade-2);
    color: inherit;
    padding: 0.4rem 0.6rem;
    font-family: inherit;
  }
</style>
