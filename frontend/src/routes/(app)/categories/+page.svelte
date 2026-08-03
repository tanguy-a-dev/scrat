<script lang="ts">
  import { onMount } from "svelte";
  import { api, buildCategoryOptions, type CategoryDto } from "$lib/api";
  import CategoryCard from "$lib/CategoryCard.svelte";
  import SearchSelect from "$lib/SearchSelect.svelte";
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

  function handleSetIcon(id: string, icon: string) {
    withErrorHandling(() => api.setCategoryIcon(id, icon));
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

  function autofocus(node: HTMLElement) {
    node.focus();
  }

  let rootCategories = $derived(categories.filter((c) => c.parent_id === null));
  let subcategoryCount = $derived(categories.length - rootCategories.length);

  function childrenOf(parent: CategoryDto): CategoryDto[] {
    return categories.filter((c) => c.parent_id === parent.id);
  }

  /* Same eligibility rule as before — never the category being deleted, and if
     it has subcategories only top-level targets — but labelled "Parent > Child"
     so a subcategory target isn't ambiguous. */
  let reassignOptions = $derived.by(() => {
    if (!pendingDelete) return [];
    const target = pendingDelete.category;
    const targetHasChildren = categories.some((c) => c.parent_id === target.id);
    const eligible = new Set(
      categories
        .filter(
          (c) =>
            c.id !== target.id && (!targetHasChildren || c.parent_id === null),
        )
        .map((c) => c.id),
    );
    return buildCategoryOptions(categories).filter((o) => eligible.has(o.id));
  });
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && pendingDelete) pendingDelete = null;
  }}
/>

<header class="page-header">
  <div class="title">
    <h1>Categories</h1>
    {#if !loading && rootCategories.length > 0}
      <span class="summary">
        {rootCategories.length} categories · {subcategoryCount} subcategories
      </span>
    {/if}
  </div>
  <form class="create-form" onsubmit={handleAddRoot}>
    <input placeholder="New category name" bind:value={newRootName} />
    <button type="submit">Add category</button>
  </form>
</header>

{#if error}<p class="error">{error}</p>{/if}

{#if loading}
  <p>Loading…</p>
{:else if rootCategories.length === 0}
  <p class="empty">No categories yet — add one above.</p>
{:else}
  <div class="grid">
    {#each rootCategories as category (category.id)}
      <CategoryCard
        {category}
        subcategories={childrenOf(category)}
        onRename={handleRename}
        onDelete={handleDelete}
        onAddChild={handleAddChild}
        onSetIcon={handleSetIcon}
      />
    {/each}
  </div>
{/if}

{#if pendingDelete}
  <!-- Modal rather than a panel appended below the grid: in a multi-column
       layout, a prompt at the bottom of the page is easy to miss entirely. -->
  <div class="overlay">
    <div
      class="reassign-panel"
      role="dialog"
      aria-modal="true"
      aria-label="Reassign transactions before deleting"
      tabindex="-1"
      use:autofocus
    >
      <p>
        "{pendingDelete.category.name}" still has transactions. Choose a
        category to move them to before deleting:
      </p>
      <div class="reassign-target">
        <SearchSelect
          options={reassignOptions}
          value={reassignTarget}
          onChange={(id) => (reassignTarget = id)}
          placeholder="Select a category…"
          searchPlaceholder="Search category…"
        />
      </div>
      <div class="actions">
        <button
          type="button"
          class="ghost"
          onclick={() => (pendingDelete = null)}
        >
          Cancel
        </button>
        <button
          type="button"
          class="danger"
          onclick={confirmReassignDelete}
          disabled={!reassignTarget}
        >
          Reassign &amp; delete
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .page-header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 1rem;
    margin-bottom: 1.5rem;
  }

  .title {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
  }

  h1 {
    margin: 0;
  }

  .summary {
    font-size: 0.85rem;
    opacity: 0.6;
    white-space: nowrap;
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

  /* Cards flow into columns instead of one tall single-file list: with ~20
     categories the whole taxonomy fits on one screen, so subcategories can be
     compared side by side rather than by scrolling. Grid's default stretch
     (no align-items override) makes every card in a row match the row's
     tallest card; CategoryCard.svelte fills that height and grows its
     subcategory area to absorb the extra space.

     The 22.5rem track floor is tuned against the nav rail, not picked for
     looks: hovering the rail widens it 4.75rem -> 8.5rem, so `main` loses
     3.75rem of width mid-interaction. At the previous 19rem floor that swing
     straddled a column boundary at 1728px (5 columns collapsed, 4 expanded)
     and again at 1440px (4 -> 3), so just moving the pointer toward the menu
     reshuffled every card. auto-fill boundaries are 3.75rem-wide danger zones
     wherever they land, so this floor doesn't remove them — it relocates them:
     at 22.5rem the column count holds steady across the rail swing at 1366,
     1440, 1512, 1600, 1680, 1728, 1792, 1920, 2056 and 2560px, leaving only
     narrow ~1280px windows unstable. Don't retune this without re-checking the
     count at both nav states across that range. */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(22.5rem, 1fr));
    gap: 0.75rem;
  }

  .overlay {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    background-color: rgba(0, 0, 0, 0.6);
  }

  .reassign-panel {
    width: 100%;
    max-width: 28rem;
    padding: 1.25rem;
    border-radius: 12px;
    border: 1px solid var(--color-shade-3);
    background-color: var(--color-shade-2);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .reassign-panel p {
    margin: 0;
  }

  .reassign-target :global(.category-select) {
    max-width: none;
    width: 100%;
  }

  .reassign-target :global(.trigger) {
    width: 100%;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  /* Cancel stays secondary: the emphasised button in a destructive dialog
     should be the one you opened the dialog to do. */
  .reassign-panel button.ghost {
    background-color: transparent;
    border: 1px solid var(--color-shade-4);
    color: inherit;
  }

  .reassign-panel button.ghost:hover {
    background-color: var(--color-shade-3);
  }

  .reassign-panel button.danger {
    background-color: var(--color-danger);
    color: var(--color-text);
  }

  .reassign-panel button.danger:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
