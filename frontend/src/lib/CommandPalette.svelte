<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { navPages, adjacentPageHref } from "$lib/navigation";

  interface Command {
    id: string;
    label: string;
    section: "Navigate" | "Actions";
    shortcut?: string;
    action: () => void;
  }

  let open = $state(false);
  let query = $state("");
  let selectedIndex = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();

  const commands: Command[] = [
    {
      id: "nav-next-page",
      label: "Next page",
      section: "Navigate",
      shortcut: "⌘/⌥ ↓",
      action: () => goto(adjacentPageHref(page.url.pathname, 1)),
    },
    {
      id: "nav-previous-page",
      label: "Previous page",
      section: "Navigate",
      shortcut: "⌘/⌥ ↑",
      action: () => goto(adjacentPageHref(page.url.pathname, -1)),
    },
    ...navPages.map((p) => ({
      id: `nav-${p.href}`,
      label: `Go to ${p.label}`,
      section: "Navigate" as const,
      action: () => goto(p.href),
    })),
    {
      id: "action-add-transaction",
      label: "Add transaction",
      section: "Actions",
      action: () => goto("/transactions?action=add-transaction"),
    },
    {
      id: "action-import-csv",
      label: "Import CSV",
      section: "Actions",
      action: () => goto("/transactions?action=import-csv"),
    },
  ];

  function isEditableTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;
  }

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter((c) => c.label.toLowerCase().includes(q));
  });

  function close() {
    open = false;
    query = "";
    selectedIndex = 0;
  }

  function run(command: Command) {
    close();
    command.action();
  }

  function handleGlobalKeydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      open = true;
      queueMicrotask(() => inputEl?.focus());
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      close();
    } else if (
      !open &&
      (event.metaKey || event.altKey) &&
      (event.key === "ArrowDown" || event.key === "ArrowUp") &&
      !isEditableTarget(event.target)
    ) {
      event.preventDefault();
      goto(adjacentPageHref(page.url.pathname, event.key === "ArrowDown" ? 1 : -1));
    }
  }

  function handleInputKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, filtered.length - 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const command = filtered[selectedIndex];
      if (command) run(command);
    }
  }

  $effect(() => {
    query;
    selectedIndex = 0;
  });

  onMount(() => {
    window.addEventListener("keydown", handleGlobalKeydown);
    return () => window.removeEventListener("keydown", handleGlobalKeydown);
  });
</script>

{#if open}
  <div
    class="backdrop"
    role="button"
    tabindex="-1"
    onclick={close}
    onkeydown={(e) => e.key === "Escape" && close()}
  >
    <div
      class="palette"
      role="presentation"
      onclick={(e) => e.stopPropagation()}
    >
      <input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={handleInputKeydown}
        placeholder="Type a command or search…"
      />
      <ul class="results">
        {#each filtered as command, i (command.id)}
          {@const showHeader = i === 0 || filtered[i - 1].section !== command.section}
          {#if showHeader}
            <li class="section-header">{command.section}</li>
          {/if}
          <li>
            <button
              type="button"
              class:selected={i === selectedIndex}
              onmouseenter={() => (selectedIndex = i)}
              onclick={() => run(command)}
            >
              <span>{command.label}</span>
              {#if command.shortcut}
                <span class="shortcut">{command.shortcut}</span>
              {/if}
            </button>
          </li>
        {:else}
          <li class="empty">No matching commands.</li>
        {/each}
      </ul>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 12vh;
    z-index: 1500;
  }

  .palette {
    background: var(--color-shade-2);
    color: inherit;
    border-radius: 12px;
    border: 1px solid var(--color-shade-3);
    width: min(32rem, 90vw);
    max-height: 60vh;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  }

  input {
    border: none;
    border-bottom: 1px solid var(--color-shade-3);
    background: transparent;
    color: inherit;
    padding: 0.9rem 1rem;
    font-size: 1rem;
    font-family: inherit;
  }

  input:focus {
    outline: none;
  }

  .results {
    list-style: none;
    margin: 0;
    padding: 0.4rem;
    overflow-y: auto;
  }

  .section-header {
    padding: 0.5rem 0.6rem 0.25rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    opacity: 0.6;
  }

  .empty {
    padding: 0.75rem 0.6rem;
    opacity: 0.7;
    font-size: 0.9rem;
  }

  button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: inherit;
    padding: 0.55rem 0.6rem;
    border-radius: 6px;
    font-size: 0.9rem;
    font-family: inherit;
    cursor: pointer;
  }

  button.selected {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
  }

  .shortcut {
    font-size: 0.75rem;
    opacity: 0.6;
    white-space: nowrap;
  }
</style>
