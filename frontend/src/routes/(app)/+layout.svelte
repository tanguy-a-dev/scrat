<script lang="ts">
  import { page } from "$app/state";
  import type { Snippet } from "svelte";

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

<style>
  .shell {
    display: flex;
    min-height: 100vh;
  }

  nav {
    width: 12rem;
    flex-shrink: 0;
    padding: 1.25rem 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    background-color: var(--color-shade-2);
    border-right: 1px solid var(--color-shade-3);
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
    image-rendering: pixelated;
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
    opacity: 0.75;
  }

  a:hover {
    opacity: 1;
    background-color: var(--color-shade-3);
  }

  a.active {
    opacity: 1;
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
  }

  main {
    flex: 1;
    padding: 2rem;
    overflow-y: auto;
  }
</style>
