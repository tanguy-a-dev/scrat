<script lang="ts">
  import "../app.css";
  import type { Snippet } from "svelte";
  import { onMount } from "svelte";

  let { children }: { children: Snippet } = $props();

  // The CSV import dropzone (ImportCsvDialog) handles its own drag/drop and
  // calls preventDefault there. Everywhere else in the app, the browser's
  // default action for a dropped file is to navigate to it / render its raw
  // text — block that globally so dragging a file over the wrong part of the
  // window can't dump file contents into the app.
  function suppressDefaultFileDrop(event: DragEvent) {
    event.preventDefault();
  }

  onMount(() => {
    window.addEventListener("dragover", suppressDefaultFileDrop);
    window.addEventListener("drop", suppressDefaultFileDrop);
    return () => {
      window.removeEventListener("dragover", suppressDefaultFileDrop);
      window.removeEventListener("drop", suppressDefaultFileDrop);
    };
  });
</script>

{@render children()}
