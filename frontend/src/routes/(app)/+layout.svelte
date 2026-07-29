<script lang="ts">
  import { page } from "$app/state";
  import type { Snippet } from "svelte";
  import FindInPage from "$lib/FindInPage.svelte";
  import Toast from "$lib/Toast.svelte";
  import CommandPalette from "$lib/CommandPalette.svelte";

  let { children }: { children: Snippet } = $props();

  const links = [
    { href: "/overview", label: "Overview" },
    { href: "/details", label: "Details" },
    { href: "/transactions", label: "Transactions" },
    { href: "/accounts", label: "Accounts" },
    { href: "/categories", label: "Categories" },
    { href: "/settings", label: "Settings" },
  ];
</script>

<div class="shell">
  <nav>
    <div class="brand">
      <img src="/favicon.png" alt="Scrat" />
      <span>Scrat</span>
    </div>
    <ul>
      {#each links as link (link.href)}
        <li>
          <a href={link.href} class:active={page.url.pathname === link.href}>
            {link.label}
          </a>
        </li>
      {/each}
    </ul>
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
    width: 3.5rem;
    overflow: hidden;
    z-index: 100;
    padding: 1.25rem 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    background-color: var(--color-shade-2);
    border-right: 1px solid var(--color-shade-3);
    transition: width 0.15s ease;
  }

  nav:hover,
  nav:focus-within {
    width: 12rem;
    box-shadow: 2px 0 12px rgba(0, 0, 0, 0.4);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0 0.5rem;
    font-weight: 700;
  }

  .brand img {
    width: 1.75rem;
    height: 1.75rem;
    flex-shrink: 0;
    image-rendering: pixelated;
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
    white-space: nowrap;
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

  main {
    flex: 1;
    margin-left: 3.5rem;
    padding: 2rem;
    overflow-y: auto;
  }
</style>
