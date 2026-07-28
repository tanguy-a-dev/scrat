<script lang="ts">
  import { onMount } from "svelte";
  import { message } from "@tauri-apps/plugin-dialog";
  import {
    api,
    buildCategoryOptions,
    formatMoney,
    type AccountDto,
    type CategoryDto,
    type ImportPreviewDto,
  } from "./api";

  let {
    accounts,
    categories,
    onImported,
    onClose,
  }: {
    accounts: AccountDto[];
    categories: CategoryDto[];
    onImported: () => void;
    onClose: () => void;
  } = $props();

  let preview = $state<ImportPreviewDto | null>(null);
  let included = $state<boolean[]>([]);
  let selectedCategoryId = $state("");
  let selectedAccountId = $state("");
  let importing = $state(false);
  let dragOver = $state(false);

  let categoryOptions = $derived(buildCategoryOptions(categories));

  let includableCount = $derived(
    preview?.rows.filter((r, i) => included[i] && r.date && r.amount_minor_units).length ?? 0,
  );

  function formatSignedAmount(minorUnits: number): string {
    const sign = minorUnits < 0 ? "-" : "";
    return `${sign}${formatMoney(Math.abs(minorUnits))}`;
  }

  async function loadBytes(bytes: number[]) {
    preview = null;
    try {
      const result = await api.previewCsvImport(bytes);
      preview = result;
      included = result.rows.map((r) => r.include_by_default);
      if (result.rows.length > 0) {
        const suggested = await api
          .suggestAccountForSource(result.rows.find((r) => r.source)?.source ?? "")
          .catch(() => null);
        if (suggested) selectedAccountId = suggested;
      }
    } catch (e) {
      await message(String(e), { title: "Import CSV", kind: "error" });
    }
  }

  async function loadFile(file: File) {
    const buffer = await file.arrayBuffer();
    await loadBytes(Array.from(new Uint8Array(buffer)));
  }

  function loadText(text: string) {
    return loadBytes(Array.from(new TextEncoder().encode(text)));
  }

  function handleFileChange(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (file) loadFile(file);
  }

  function handleDragOver(event: DragEvent) {
    event.preventDefault();
    dragOver = true;
  }

  function handleDragLeave() {
    dragOver = false;
  }

  function handleDrop(event: DragEvent) {
    event.preventDefault();
    dragOver = false;
    const file = event.dataTransfer?.files?.[0];
    if (file) loadFile(file);
  }

  function handlePaste(event: ClipboardEvent) {
    if (preview) return;
    const text = event.clipboardData?.getData("text/plain");
    if (!text?.trim()) return;
    event.preventDefault();
    loadText(text);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }

  onMount(() => {
    window.addEventListener("paste", handlePaste);
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("paste", handlePaste);
      window.removeEventListener("keydown", handleKeydown);
    };
  });

  async function handleImport() {
    if (!preview) return;
    importing = true;
    try {
      const rows: {
        date: string;
        amount_minor_units: number;
        source: string;
        category: string | null;
      }[] = [];
      preview.rows.forEach((r, i) => {
        if (included[i] && r.date !== null && r.amount_minor_units !== null) {
          rows.push({
            date: r.date,
            amount_minor_units: r.amount_minor_units,
            source: r.source,
            category: r.csv_category,
          });
        }
      });
      await api.commitCsvImport(
        rows,
        selectedCategoryId || null,
        selectedAccountId || null,
      );
      onImported();
      onClose();
    } catch (e) {
      await message(String(e), { title: "Import CSV", kind: "error" });
    } finally {
      importing = false;
    }
  }
</script>

<div class="backdrop">
  <div class="dialog">
    <h2>Import transactions from CSV</h2>

    {#if !preview}
      <p class="hint">
        Pick a bank export file, drag one in, or paste CSV content (⌘V /
        Ctrl+V) — the format is detected automatically, no header row
        required.
      </p>
      <div
        class="dropzone"
        class:drag-over={dragOver}
        role="group"
        aria-label="CSV file drop zone"
        ondragover={handleDragOver}
        ondragleave={handleDragLeave}
        ondrop={handleDrop}
      >
        <input type="file" accept=".csv,text/csv" onchange={handleFileChange} />
        <p class="dropzone-hint">or drop a .csv file here</p>
      </div>
      <button type="button" onclick={onClose}>Cancel</button>
    {:else}
      <p class="hint">
        Detected: date column ({Math.round(preview.date_confidence * 100)}%
        confidence), amount column ({Math.round(preview.amount_confidence * 100)}%
        confidence). Uncheck any row that isn't a real transaction (e.g. an
        opening/closing balance line). "Category" is the category column from
        the file itself, if it has one — each row with one is filed under a
        matching category (creating it if it doesn't exist yet). Rows without
        one use the fallback chosen below, or "Other Income" if you leave that
        unset.
        Leave the account unset to use your default account.
      </p>

      <div class="targets">
        <select bind:value={selectedCategoryId}>
          <option value="">Fallback category (optional)…</option>
          {#each categoryOptions as c (c.id)}
            <option value={c.id}>{c.label}</option>
          {/each}
        </select>
        <select bind:value={selectedAccountId}>
          <option value="">Destination account (optional)…</option>
          {#each accounts as a (a.id)}
            <option value={a.id}>{a.name}{a.is_default ? " (default)" : ""}</option>
          {/each}
        </select>
      </div>

      <div class="rows">
        <table>
          <thead>
            <tr>
              <th></th>
              <th>Date</th>
              <th>Amount</th>
              <th>Source</th>
              <th>Category</th>
            </tr>
          </thead>
          <tbody>
            {#each preview.rows as row, i (i)}
              {@const invalid = row.date === null || row.amount_minor_units === null}
              <tr class:invalid>
                <td>
                  <input
                    type="checkbox"
                    bind:checked={included[i]}
                    disabled={invalid}
                  />
                </td>
                <td>{row.date ?? "—"}</td>
                <td
                  >{row.amount_minor_units !== null
                    ? formatSignedAmount(row.amount_minor_units)
                    : "—"}</td
                >
                <td>{row.source || "—"}</td>
                <td class="suggestion">{row.csv_category ?? "—"}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <div class="actions">
        <button type="button" onclick={onClose}>Cancel</button>
        <button
          type="button"
          disabled={importing || includableCount === 0}
          onclick={handleImport}
        >
          Import {includableCount} transaction{includableCount === 1 ? "" : "s"}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .dialog {
    background: var(--color-shade-2);
    color: inherit;
    border-radius: 12px;
    padding: 1.5rem;
    width: min(40rem, 90vw);
    max-height: 85vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  h2 {
    margin: 0;
  }

  .hint {
    opacity: 0.8;
    font-size: 0.9rem;
  }

  .targets {
    display: flex;
    gap: 0.5rem;
  }

  select,
  input[type="file"] {
    border-radius: 6px;
    border: 1px solid var(--color-shade-3);
    background-color: var(--color-shade-1);
    color: inherit;
    padding: 0.4rem 0.6rem;
    font-family: inherit;
  }

  .dropzone {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 1.25rem;
    border: 2px dashed var(--color-shade-4);
    border-radius: 10px;
    transition: border-color 0.15s, background-color 0.15s;
  }

  .dropzone.drag-over {
    border-color: var(--color-accent);
    background-color: color-mix(in srgb, var(--color-accent) 15%, transparent);
  }

  .dropzone-hint {
    margin: 0;
    font-size: 0.85rem;
    opacity: 0.7;
  }

  .rows {
    max-height: 20rem;
    overflow-y: auto;
    border: 1px solid var(--color-shade-3);
    border-radius: 8px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.85rem;
  }

  th,
  td {
    text-align: left;
    padding: 0.35rem 0.5rem;
  }

  tr.invalid {
    opacity: 0.5;
  }

  .suggestion {
    font-style: italic;
    opacity: 0.75;
  }

  tbody tr:not(:last-child) {
    border-bottom: 1px solid var(--color-shade-3);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  button {
    border-radius: 6px;
    border: none;
    padding: 0.5rem 1rem;
    cursor: pointer;
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
    font-size: 0.9rem;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
