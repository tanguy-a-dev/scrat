export interface NavPage {
  href: string;
  label: string;
}

/* Single source of truth for the app's page order — both the side nav and
   the cmd/alt+arrow "next/previous page" shortcut walk this same list, so
   they can never drift out of sync with each other. */
export const navPages: NavPage[] = [
  { href: "/overview", label: "Overview" },
  { href: "/details", label: "Details" },
  { href: "/transactions", label: "Transactions" },
  { href: "/accounts", label: "Accounts" },
  { href: "/categories", label: "Categories" },
  { href: "/settings", label: "Settings" },
];

export function adjacentPageHref(currentPathname: string, delta: 1 | -1): string {
  const currentIndex = navPages.findIndex((p) => p.href === currentPathname);
  const baseIndex = currentIndex === -1 ? 0 : currentIndex;
  const nextIndex = (baseIndex + delta + navPages.length) % navPages.length;
  return navPages[nextIndex].href;
}
