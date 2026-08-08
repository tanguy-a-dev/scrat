<script lang="ts">
  import { onMount } from "svelte";
  import { afterNavigate } from "$app/navigation";
  import { ChevronUp, ChevronDown, X } from "@lucide/svelte";
  import { t } from "$lib/i18n.svelte";

  const supported = typeof CSS !== "undefined" && "highlights" in CSS;

  let open = $state(false);
  let query = $state("");
  let matchCount = $state(0);
  let currentIndex = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();

  /* Category/account names live as the `value` of an <input> (they're
     inline-editable), not as DOM text nodes — a TreeWalker over text never
     sees them. So a match is either a Range in ordinary text, or a span
     inside an editable input's value, each highlighted a different way. */
  type TextMatch = { kind: "text"; range: Range };
  type InputMatch = { kind: "input"; el: HTMLInputElement; start: number; end: number };
  type Match = TextMatch | InputMatch;

  let matches: Match[] = [];
  let markedInputs = new Set<HTMLInputElement>();

  function collectTextMatches(term: string): TextMatch[] {
    const found: TextMatch[] = [];
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
        found.push({ kind: "text", range });
        from = at + term.length;
      }
    }
    return found;
  }

  function collectInputMatches(term: string): InputMatch[] {
    const found: InputMatch[] = [];
    for (const el of document.querySelectorAll<HTMLInputElement>("input")) {
      if (el.type !== "text" || el.closest(".find-in-page-bar")) continue;
      const value = el.value;
      if (!value) continue;
      const lower = value.toLowerCase();
      let from = 0;
      let at: number;
      while ((at = lower.indexOf(term, from)) !== -1) {
        found.push({ kind: "input", el, start: at, end: at + term.length });
        from = at + term.length;
      }
    }
    return found;
  }

  function comparePosition(a: Match, b: Match): number {
    const nodeA = a.kind === "text" ? a.range.startContainer : a.el;
    const nodeB = b.kind === "text" ? b.range.startContainer : b.el;
    if (nodeA === nodeB) {
      return a.kind === "text" && b.kind === "text"
        ? a.range.startOffset - b.range.startOffset
        : 0;
    }
    return nodeA.compareDocumentPosition(nodeB) & Node.DOCUMENT_POSITION_FOLLOWING
      ? -1
      : 1;
  }

  function clearInputMarks() {
    for (const el of markedInputs) {
      el.classList.remove("find-in-page-input-match", "find-in-page-input-current");
    }
    markedInputs.clear();
  }

  function paintHighlights() {
    if (supported) {
      /* WebKit (the WKWebView engine Tauri uses on macOS) doesn't always
         repaint regions that drop out of a Highlight when .set() is called
         again on the same registry key — a search narrowing from "e" to
         "expense" can leave the old single-letter matches visually stuck.
         Deleting before re-setting forces a full repaint instead of relying
         on the engine to diff the old and new range lists. Chromium doesn't
         need this, but it's harmless there. */
      CSS.highlights.delete("find-in-page-all");
      const textRanges = matches
        .filter((m): m is TextMatch => m.kind === "text")
        .map((m) => m.range);
      if (textRanges.length > 0) {
        CSS.highlights.set("find-in-page-all", new Highlight(...textRanges));
      }
    }

    clearInputMarks();
    for (const m of matches) {
      if (m.kind === "input") {
        m.el.classList.add("find-in-page-input-match");
        markedInputs.add(m.el);
      }
    }

    const current = matches[currentIndex];
    if (supported) CSS.highlights.delete("find-in-page-current");
    if (current?.kind === "text") {
      if (supported) CSS.highlights.set("find-in-page-current", new Highlight(current.range));
    } else if (current?.kind === "input") {
      current.el.classList.add("find-in-page-input-current");
      current.el.setSelectionRange(current.start, current.end);
    }
  }

  function clearHighlights() {
    if (supported) {
      CSS.highlights.delete("find-in-page-all");
      CSS.highlights.delete("find-in-page-current");
    }
    clearInputMarks();
  }

  function scrollToCurrent() {
    const current = matches[currentIndex];
    if (!current) return;
    const el = current.kind === "text" ? current.range.startContainer.parentElement : current.el;
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
  }

  function runSearch() {
    currentIndex = 0;
    const term = query.trim().toLowerCase();
    matches = term
      ? [
          ...(supported ? collectTextMatches(term) : []),
          ...collectInputMatches(term),
        ].sort(comparePosition)
      : [];
    matchCount = matches.length;
    paintHighlights();
    if (matchCount > 0) scrollToCurrent();
  }

  function step(delta: 1 | -1) {
    if (matches.length === 0) return;
    currentIndex = (currentIndex + delta + matches.length) % matches.length;
    paintHighlights();
    scrollToCurrent();
  }

  function close() {
    open = false;
    query = "";
    matches = [];
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
      placeholder={t("find.placeholder")}
    />
    <span class="count">
      {#if !supported}
        {t("find.unsupported")}
      {:else if query.trim() === ""}
        &nbsp;
      {:else if matchCount === 0}
        {t("find.noResults")}
      {:else}
        {currentIndex + 1} / {matchCount}
      {/if}
    </span>
    <button
      type="button"
      class="icon-button"
      onclick={() => step(-1)}
      disabled={matchCount === 0}
      aria-label={t("find.previousMatch")}
    >
      <ChevronUp size={16} />
    </button>
    <button
      type="button"
      class="icon-button"
      onclick={() => step(1)}
      disabled={matchCount === 0}
      aria-label={t("find.nextMatch")}
    >
      <ChevronDown size={16} />
    </button>
    <button
      type="button"
      class="icon-button"
      onclick={close}
      aria-label={t("find.close")}
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

  /* Applied imperatively to arbitrary <input> elements elsewhere in the app
     (category/account name fields) — those live in other components'
     templates, so these rules must be :global to reach them. Mirrors the
     dim/bright pairing of the ::highlight() rules in app.css, since inputs
     can't be targeted by the CSS Custom Highlight API used for plain text. */
  :global(.find-in-page-input-match) {
    outline: 2px solid color-mix(in srgb, #ffd500 55%, transparent);
    outline-offset: 1px;
  }

  :global(.find-in-page-input-current) {
    outline: 2px solid rgba(255, 140, 0, 0.9);
    outline-offset: 1px;
  }
</style>
