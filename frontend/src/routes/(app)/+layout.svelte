<script lang="ts">
  import { page } from "$app/state";
  import type { Snippet } from "svelte";
  import FindInPage from "$lib/FindInPage.svelte";
  import Toast from "$lib/Toast.svelte";
  import CommandPalette from "$lib/CommandPalette.svelte";
  import { navPages } from "$lib/navigation";
  import { session } from "$lib/session.svelte";
  import { t } from "$lib/i18n.svelte";

  let { children }: { children: Snippet } = $props();
</script>

<div class="shell">
  <nav>
    <div class="brand">
      <img src="/favicon.png" alt="Scrat" />
      <span>Scrat</span>
    </div>
    <ul>
      {#each navPages as link (link.href)}
        <li>
          <a href={link.href} class:active={page.url.pathname === link.href}>
            {t(link.labelKey)}
          </a>
        </li>
      {/each}
    </ul>
    <button class="lock" onclick={() => session.lock()}>{t("nav.lock")}</button>
  </nav>
  <main>
    {@render children()}
  </main>
</div>

<FindInPage />
<Toast />
<CommandPalette />

<style>
  .shell {
    display: flex;
    min-height: 100vh;
  }

  nav {
    position: fixed;
    top: 0;
    left: 0;
    bottom: 0;
    width: 4.75rem;
    overflow: hidden;
    z-index: 100;
    padding: 1.25rem 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    background-color: transparent;
    border-right: 1px solid transparent;
    transition:
      width 0.15s ease,
      background-color 0.15s ease,
      border-color 0.15s ease;
  }

  nav:hover,
  nav:focus-within {
    width: 8.5rem;
    background-color: var(--color-shade-2);
    border-right: 1px solid var(--color-shade-3);
    box-shadow: 2px 0 12px rgba(0, 0, 0, 0.4);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    font-weight: 700;
  }

  .brand img {
    width: 2.625rem;
    height: 2.625rem;
    flex-shrink: 0;
    image-rendering: pixelated;
    transition:
      width 0.15s ease,
      height 0.15s ease;
  }

  nav:hover .brand img,
  nav:focus-within .brand img {
    width: 1.75rem;
    height: 1.75rem;
  }

  .brand span {
    white-space: nowrap;
    opacity: 0;
    transition: opacity 0.1s ease;
  }

  nav:hover .brand span,
  nav:focus-within .brand span {
    opacity: 1;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  a {
    display: block;
    padding: 0.5rem 0.75rem;
    border-radius: 6px;
    text-decoration: none;
    color: inherit;
    opacity: 0;
    line-height: 1.2;
    transition: opacity 0.1s ease;
  }

  nav:hover a,
  nav:focus-within a {
    opacity: 0.75;
  }

  nav:hover a:hover,
  nav:hover a.active,
  nav:focus-within a:hover,
  nav:focus-within a.active {
    opacity: 1;
  }

  a:hover {
    background-color: var(--color-shade-3);
  }

  a.active {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
  }

  .lock {
    margin-top: auto;
    padding: 0.5rem 0.75rem;
    border: none;
    border-radius: 6px;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    white-space: nowrap;
    cursor: pointer;
    opacity: 0;
    line-height: 1.2;
    transition: opacity 0.1s ease;
  }

  nav:hover .lock,
  nav:focus-within .lock {
    opacity: 0.75;
  }

  nav:hover .lock:hover,
  nav:focus-within .lock:hover {
    opacity: 1;
    background-color: var(--color-shade-3);
  }

  main {
    flex: 1;
    margin-left: 4.75rem;
    padding: 2rem;
    overflow-y: auto;
    transition: margin-left 0.15s ease;
  }

  .shell:has(nav:hover) main,
  .shell:has(nav:focus-within) main {
    margin-left: 8.5rem;
  }
</style>
