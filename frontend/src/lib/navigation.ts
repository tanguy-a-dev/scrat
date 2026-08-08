import type { MessageKey } from "$lib/i18n.svelte";

export interface NavPage {
  href: string;
  /** A dictionary key, not a label. The nav is rendered in the interface
   * language, and this list is a module-level constant — resolving the text
   * here would freeze it at whatever the language was on first import. */
  labelKey: MessageKey;
}

/* Single source of truth for the app's page order — both the side nav and
   the cmd/alt+arrow "next/previous page" shortcut walk this same list, so
   they can never drift out of sync with each other. */
export const navPages: NavPage[] = [
  { href: "/overview", labelKey: "nav.overview" },
  { href: "/details", labelKey: "nav.details" },
  { href: "/transactions", labelKey: "nav.transactions" },
  { href: "/accounts", labelKey: "nav.accounts" },
  { href: "/categories", labelKey: "nav.categories" },
  { href: "/settings", labelKey: "nav.settings" },
];

export function adjacentPageHref(currentPathname: string, delta: 1 | -1): string {
  const currentIndex = navPages.findIndex((p) => p.href === currentPathname);
  const baseIndex = currentIndex === -1 ? 0 : currentIndex;
  const nextIndex = (baseIndex + delta + navPages.length) % navPages.length;
  return navPages[nextIndex].href;
}
