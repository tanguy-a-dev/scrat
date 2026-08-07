import { defineConfig } from "vitest/config";
import { sveltekit } from "@sveltejs/kit/vite";

/* Kept separate from `vite.config.ts` so the dev-server settings Tauri
   depends on (fixed port 1420, strictPort, the src-tauri watch exclusion)
   don't apply to a test run, and so a broken test config can never affect
   `npm run dev` or `npm run build`.

   The SvelteKit plugin is loaded here purely for module resolution: it's what
   makes `$lib/*` and `$app/*` resolve the same way they do in the real app,
   so a test imports the exact module the app does rather than a copy wired up
   differently. It also compiles `.svelte.ts` files, which is what lets a rune
   module like `session.svelte.ts` be tested at all. */
export default defineConfig({
  plugins: [sveltekit()],
  test: {
    /* jsdom, not node: the modules under test reach for `window` (activity
       listeners, `matchMedia`) and would otherwise have to be written around
       an environment the app never actually runs in. */
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    /* The IPC-drift test reads the Rust sources, which live outside this
       package — everything else stays inside `src`. */
    root: ".",
  },
  resolve: {
    /* Svelte ships separate server and browser entrypoints; without this,
       jsdom tests resolve the SSR build and runes behave differently than
       they do in the app. */
    conditions: ["browser"],
  },
});
