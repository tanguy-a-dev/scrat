<script lang="ts">
  /** A dropdown you can type into, for lists too long or too nested to scan
   * in a native `<select>`.
   *
   * Named for what it does rather than what it picks: it started as the
   * category picker, but the account list wants the same control, and a
   * component called `CategorySelect` choosing accounts is exactly the kind
   * of name-that-lies this codebase avoids elsewhere.
   */
  import type { Snippet } from "svelte";
  import { t } from "$lib/i18n.svelte";

  interface Option {
    id: string;
    label: string;
    /** Set by `buildCategoryOptions` for a subcategory. Only `stacked` reads
     * these; every other caller works off `label` alone. */
    parentName?: string;
    name?: string;
  }

  let {
    options,
    value,
    onChange,
    placeholder = t("component.select"),
    searchPlaceholder = t("component.searchPlaceholder"),
    stacked = false,
    trigger,
  }: {
    options: Option[];
    value: string;
    onChange: (id: string) => void;
    /** Shown on the closed trigger when nothing is selected. */
    placeholder?: string;
    /** Shown in the filter box once the dropdown is open. */
    searchPlaceholder?: string;
    /** Draw the closed trigger as two lines — the parent's name above, dim
     * and small, this option's own name below — instead of one joined
     * "Parent > Child". For narrow columns, where one line has to ellipsise
     * and does it from the right, cutting off the *sub*category: the part
     * that distinguishes the option from its siblings. Stacking makes the
     * parent the line that gives way instead. Off everywhere the control has
     * a full row to itself, and it never affects the open dropdown — that
     * list stays flat, since it's scanned and typed at rather than read a
     * row at a time. */
    stacked?: boolean;
    trigger?: Snippet<[{ label: string; active: boolean }]>;
  } = $props();

  let open = $state(false);
  let query = $state("");
  let highlighted = $state(0);
  let containerEl: HTMLDivElement | undefined = $state();
  let inputEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLUListElement | undefined = $state();

  let selected = $derived(options.find((o) => o.id === value));
  let selectedLabel = $derived(selected?.label ?? placeholder);
  /** Two lines only when there are two levels to show — a top-level category
   * has no parent, so it stays a single line and its row stays short. */
  let showStacked = $derived(stacked && !!selected?.parentName);

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return options;
    return options.filter((o) => o.label.toLowerCase().includes(q));
  });

  $effect(() => {
    filtered;
    highlighted = 0;
  });

  function openDropdown() {
    open = true;
    query = "";
    queueMicrotask(() => inputEl?.focus());
  }

  function close() {
    open = false;
  }

  function select(id: string) {
    onChange(id);
    close();
  }

  function handleTriggerClick() {
    if (open) close();
    else openDropdown();
  }

  function handleWindowClick(event: MouseEvent) {
    if (open && containerEl && !containerEl.contains(event.target as Node)) {
      close();
    }
  }

  function moveHighlight(delta: number) {
    if (filtered.length === 0) return;
    highlighted = Math.min(Math.max(highlighted + delta, 0), filtered.length - 1);
    queueMicrotask(() =>
      listEl
        ?.querySelector("button.highlighted")
        ?.scrollIntoView({ block: "nearest" }),
    );
  }

  function handleInputKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      moveHighlight(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      moveHighlight(-1);
    } else if (event.key === "Tab") {
      event.preventDefault();
      moveHighlight(event.shiftKey ? -1 : 1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      const option = filtered[highlighted];
      if (option) select(option.id);
    }
  }
</script>

<svelte:window onclick={handleWindowClick} />

<div
  class="search-select"
  class:icon-mode={!!trigger}
  class:open
  bind:this={containerEl}
>
  <button
    type="button"
    class="trigger"
    class:icon-trigger={!!trigger}
    class:stacked-trigger={showStacked}
    class:active={!!trigger && value !== ""}
    title={stacked ? selectedLabel : undefined}
    onclick={handleTriggerClick}
  >
    {#if trigger}
      {@render trigger({ label: selectedLabel, active: value !== "" })}
    {:else if showStacked}
      <span class="stacked-parent">{selected?.parentName}</span>
      <span class="stacked-name">{selected?.name}</span>
    {:else}
      {selectedLabel}
    {/if}
  </button>
  {#if open}
    <div class="dropdown">
      <input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={handleInputKeydown}
        placeholder={searchPlaceholder}
        spellcheck="false"
        autocomplete="off"
        autocorrect="off"
        autocapitalize="off"
      />
      <ul class="options" bind:this={listEl}>
        {#if filtered.length === 0}
          <li class="empty">{t("component.noMatches")}</li>
        {:else}
          {#each filtered as option, i (option.id)}
            <li>
              <button
                type="button"
                class:selected={option.id === value}
                class:highlighted={i === highlighted}
                onmouseenter={() => (highlighted = i)}
                onclick={() => select(option.id)}
              >
                {option.label}
              </button>
            </li>
          {/each}
        {/if}
      </ul>
    </div>
  {/if}
</div>

<style>
  .search-select {
    position: relative;
    display: inline-block;
    max-width: 11rem;
    /* Its own explicit stacking context, ranked above ordinary sibling
       content by a real z-index rather than the ambiguous "auto" level —
       a scrollable sibling (a checkbox column with overflow: auto, as the
       CSV import preview table has) can otherwise end up painted on top of
       this dropdown in WebKit, the engine Tauri uses on macOS, even though
       the dropdown's own z-index is higher. */
    isolation: isolate;
    z-index: 1;
  }

  /* The dropdown's own z-index only orders it *within* this root, so the root
     is what has to outrank the rest of the page while it's open — every other
     SearchSelect sits at z-index 1 too, and a later one in document order
     (the CSV dialog's destination-account picker, the next row's category
     picker) would otherwise paint straight over the open list. */
  .search-select.open {
    z-index: 5;
  }

  .search-select.icon-mode {
    max-width: none;
  }

  .trigger {
    width: 100%;
    border-radius: 4px;
    border: 1px solid var(--color-shade-3);
    background-color: var(--color-shade-2);
    color: inherit;
    padding: 0.2rem 0.4rem;
    font-size: 0.85rem;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .trigger.icon-trigger {
    width: auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.3rem;
  }

  /* The single-line trigger ellipsises itself; the stacked one hands that job
     to each line, so it has to stop clipping at the button level or the
     second line would be cut off vertically. */
  .trigger.stacked-trigger {
    white-space: normal;
    overflow: visible;
    line-height: 1.2;
    padding-top: 0.1rem;
    padding-bottom: 0.1rem;
  }

  .stacked-parent,
  .stacked-name {
    display: block;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Smaller and dimmer than the name below it: this is the line that should
     give way first, both to the eye and to the ellipsis. */
  .stacked-parent {
    font-size: 0.72rem;
    opacity: 0.6;
  }

  .trigger.icon-trigger.active {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
    border-color: var(--color-accent);
  }

  .dropdown {
    position: absolute;
    top: calc(100% + 0.25rem);
    left: 0;
    z-index: 100;
    width: max(100%, 16rem);
    background: var(--color-shade-2);
    border: 1px solid var(--color-shade-3);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .dropdown input {
    border: none;
    border-bottom: 1px solid var(--color-shade-3);
    background: transparent;
    color: inherit;
    padding: 0.5rem 0.6rem;
    font-size: 0.85rem;
    font-family: inherit;
  }

  .dropdown input:focus {
    outline: none;
  }

  .options {
    list-style: none;
    margin: 0;
    padding: 0.3rem;
    max-height: 14rem;
    overflow-y: auto;
  }

  .options button {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: inherit;
    padding: 0.4rem 0.5rem;
    border-radius: 6px;
    font-size: 0.85rem;
    font-family: inherit;
    cursor: pointer;
  }

  .options button.highlighted {
    background-color: var(--color-shade-3);
  }

  .options button.selected {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
  }

  .empty {
    padding: 0.5rem 0.6rem;
    opacity: 0.7;
    font-size: 0.85rem;
  }
</style>
