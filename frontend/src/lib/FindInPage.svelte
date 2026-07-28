<script lang="ts">
  import { onMount } from "svelte";
  import { afterNavigate } from "$app/navigation";
  import { ChevronUp, ChevronDown, X } from "@lucide/svelte";

  const supported = typeof CSS !== "undefined" && "highlights" in CSS;

  let open = $state(false);
  let query = $state("");
  let matchCount = $state(0);
  let currentIndex = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();

  let ranges: Range[] = [];

  function collectMatches(term: string): Range[] {
    const found: Range[] = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        const parent = (node as Text).parentElement;
        if (!parent || parent.closest(".find-in-page-bar")) {
          return NodeFilter.FILTER_REJECT;
        }
        if (["SCRIPT", "STYLE", "TEXTAREA"].includes(parent.tagName)) {
          return NodeFilter.FILTER_REJECT;
        }
        return node.textContent?.trim()
          ? NodeFilter.FILTER_ACCEPT
          : NodeFilter.FILTER_REJECT;
      },
    });

    let node: Node | null;
    while ((node = walker.nextNode())) {
      const text = (node.textContent ?? "").toLowerCase();
      let from = 0;
      let at: number;
      while ((at = text.indexOf(term, from)) !== -1) {
        const range = new Range();
        range.setStart(node, at);
        range.setEnd(node, at + term.length);
        found.push(range);
        from = at + term.length;
      }
    }
    return found;
  }

  function paintHighlights() {
    if (!supported) return;
    CSS.highlights.set("find-in-page-all", new Highlight(...ranges));
    if (ranges.length > 0) {
      CSS.highlights.set("find-in-page-current", new Highlight(ranges[currentIndex]));
    } else {
      CSS.highlights.delete("find-in-page-current");
    }
  }

  function clearHighlights() {
    if (!supported) return;
    CSS.highlights.delete("find-in-page-all");
    CSS.highlights.delete("find-in-page-current");
  }

  function scrollToCurrent() {
    const range = ranges[currentIndex];
    range?.startContainer.parentElement?.scrollIntoView({
      block: "center",
      behavior: "smooth",
    });
  }

  function runSearch() {
    currentIndex = 0;
    const term = query.trim().toLowerCase();
    ranges = term && supported ? collectMatches(term) : [];
    matchCount = ranges.length;
    paintHighlights();
    if (matchCount > 0) scrollToCurrent();
  }

  function step(delta: 1 | -1) {
    if (ranges.length === 0) return;
    currentIndex = (currentIndex + delta + ranges.length) % ranges.length;
    paintHighlights();
    scrollToCurrent();
  }

  function close() {
    open = false;
    query = "";
    ranges = [];
    matchCount = 0;
    clearHighlights();
  }

  function handleGlobalKeydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
      event.preventDefault();
      open = true;
      queueMicrotask(() => inputEl?.select());
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      close();
    }
  }

  function handleInputKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      step(event.shiftKey ? -1 : 1);
    } else if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }

  afterNavigate(() => {
    if (open) close();
  });

  onMount(() => {
    window.addEventListener("keydown", handleGlobalKeydown);
    return () => {
      window.removeEventListener("keydown", handleGlobalKeydown);
      clearHighlights();
    };
  });
</script>

{#if open}
  <div class="find-in-page-bar">
    <input
      bind:this={inputEl}
      bind:value={query}
      oninput={runSearch}
      onkeydown={handleInputKeydown}
      placeholder="Find on page…"
    />
    <span class="count">
      {#if !supported}
        unsupported
      {:else if query.trim() === ""}
        &nbsp;
      {:else if matchCount === 0}
        No results
      {:else}
        {currentIndex + 1} / {matchCount}
      {/if}
    </span>
    <button
      type="button"
      class="icon-button"
      onclick={() => step(-1)}
      disabled={matchCount === 0}
      aria-label="Previous match"
    >
      <ChevronUp size={16} />
    </button>
    <button
      type="button"
      class="icon-button"
      onclick={() => step(1)}
      disabled={matchCount === 0}
      aria-label="Next match"
    >
      <ChevronDown size={16} />
    </button>
    <button
      type="button"
      class="icon-button"
      onclick={close}
      aria-label="Close find bar"
    >
      <X size={16} />
    </button>
  </div>
{/if}

<style>
  .find-in-page-bar {
    position: fixed;
    top: 1rem;
    right: 1.5rem;
    z-index: 1000;
    display: flex;
    align-items: center;
    gap: 0.3rem;
    background-color: var(--color-shade-2);
    border: 1px solid var(--color-shade-3);
    border-radius: 8px;
    padding: 0.4rem 0.5rem;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
  }

  input {
    border: 1px solid var(--color-shade-3);
    background-color: var(--color-shade-1);
    color: inherit;
    border-radius: 5px;
    padding: 0.35rem 0.55rem;
    font-size: 0.85rem;
    font-family: inherit;
    width: 12rem;
  }

  .count {
    font-size: 0.75rem;
    opacity: 0.7;
    min-width: 4.5rem;
    text-align: center;
    white-space: nowrap;
  }
</style>
