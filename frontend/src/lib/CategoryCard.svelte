<script lang="ts">
  import type { CategoryDto } from "./api";
  import DeleteButton from "./DeleteButton.svelte";
  import { CATEGORY_ICONS, iconComponentFor } from "./categoryIcons";
  import { Plus } from "@lucide/svelte";

  let {
    category,
    subcategories,
    onRename,
    onDelete,
    onAddChild,
    onSetIcon,
  }: {
    category: CategoryDto;
    /** Direct children only — the hierarchy is never deeper than two levels. */
    subcategories: CategoryDto[];
    onRename: (id: string, name: string) => void;
    onDelete: (category: CategoryDto) => void;
    onAddChild: (parentId: string, name: string) => void;
    onSetIcon: (id: string, icon: string) => void;
  } = $props();

  let addingChild = $state(false);
  let childName = $state("");

  let CurrentIcon = $derived(iconComponentFor(category.icon));
  let iconPickerOpen = $state(false);
  let iconPickerEl: HTMLElement | undefined = $state();

  function selectIcon(key: string) {
    iconPickerOpen = false;
    if (key !== category.icon) onSetIcon(category.id, key);
  }

  $effect(() => {
    if (!iconPickerOpen) return;
    function handleClickOutside(event: MouseEvent) {
      if (iconPickerEl && !iconPickerEl.contains(event.target as Node)) {
        iconPickerOpen = false;
      }
    }
    function handleKeydown(event: KeyboardEvent) {
      if (event.key === "Escape") iconPickerOpen = false;
    }
    window.addEventListener("mousedown", handleClickOutside);
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("mousedown", handleClickOutside);
      window.removeEventListener("keydown", handleKeydown);
    };
  });

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

<section class="card">
  <header>
    <span class="icon-picker" bind:this={iconPickerEl}>
      <button
        type="button"
        class="icon-button"
        aria-label="Change icon"
        title="Change icon"
        onclick={() => (iconPickerOpen = !iconPickerOpen)}
      >
        <CurrentIcon size={16} />
      </button>
      {#if iconPickerOpen}
        <div class="icon-popover" role="menu">
          {#each CATEGORY_ICONS as option (option.key)}
            <button
              type="button"
              class="icon-option"
              class:selected={option.key === category.icon}
              aria-label={option.key}
              title={option.key}
              onclick={() => selectIcon(option.key)}
            >
              <option.component size={16} />
            </button>
          {/each}
        </div>
      {/if}
    </span>
    <input
      class="name"
      value={category.name}
      readonly={category.is_default}
      aria-label="Category name"
      onchange={(e) => onRename(category.id, e.currentTarget.value)}
    />
    {#if !category.is_default}
      <span class="dim">
        <DeleteButton
          label="Delete category"
          onConfirm={() => onDelete(category)}
        />
      </span>
    {/if}
  </header>

  <div class="subs">
    {#each subcategories as child (child.id)}
      <span class="chip">
        <input
          class="chip-name"
          value={draftFor(child)}
          size={Math.max(draftFor(child).length + 1, 4)}
          readonly={child.is_default}
          aria-label="Subcategory name"
          oninput={(e) => (drafts[child.id] = e.currentTarget.value)}
          onchange={(e) => onRename(child.id, e.currentTarget.value)}
        />
        {#if !child.is_default}
          <DeleteButton
            label="Delete subcategory"
            compact
            onConfirm={() => onDelete(child)}
          />
        {/if}
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
    height: 100%;
    gap: 0.6rem;
    padding: 0.85rem 0.9rem;
    border-radius: 12px;
    background-color: var(--color-shade-2);
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .icon-picker {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
  }

  .icon-popover {
    position: absolute;
    top: calc(100% + 0.4rem);
    left: 0;
    z-index: 20;
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 0.3rem;
    padding: 0.5rem;
    border-radius: 10px;
    background-color: var(--color-shade-3);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }

  .icon-option {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.9rem;
    height: 1.9rem;
    padding: 0;
    border: none;
    border-radius: 50%;
    background-color: var(--color-shade-2);
    color: inherit;
    cursor: pointer;
  }

  .icon-option:hover {
    background-color: var(--color-shade-4);
  }

  .icon-option.selected {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
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
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    align-content: flex-start;
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
