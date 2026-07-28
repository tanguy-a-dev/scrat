<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    buildCategoryOptions,
    formatMinorUnits,
    type AccountDto,
    type CategoryDto,
    type ImportPreviewDto,
    type ImportSummaryDto,
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

  let error = $state("");
  let preview = $state<ImportPreviewDto | null>(null);
  let included = $state<boolean[]>([]);
  let selectedCategoryId = $state("");
  let selectedAccountId = $state("");
  let summary = $state<ImportSummaryDto | null>(null);
  let importing = $state(false);
  let dragOver = $state(false);

  let categoryOptions = $derived(buildCategoryOptions(categories));
  let activeAccounts = $derived(accounts.filter((a) => a.status === "active"));

  let includableCount = $derived(
    preview?.rows.filter((r, i) => included[i] && r.date && r.amount_minor_units).length ?? 0,
  );

  async function loadBytes(bytes: number[]) {
    error = "";
    summary = null;
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
      error = String(e);
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
    if (preview || summary) return;
    const text = event.clipboardData?.getData("text/plain");
    if (!text?.trim()) return;
    event.preventDefault();
    loadText(text);
  }

  onMount(() => {
    window.addEventListener("paste", handlePaste);
    return () => window.removeEventListener("paste", handlePaste);
  });

  async function handleImport() {
    if (!preview || !selectedAccountId) return;
    error = "";
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
      summary = await api.commitCsvImport(rows, selectedCategoryId || null, selectedAccountId);
      onImported();
    } catch (e) {
      error = String(e);
    } finally {
      importing = false;
    }
  }
</script>

<div class="backdrop">
  <div class="dialog">
    <h2>Import transactions from CSV</h2>

    {#if error}<p class="error">{error}</p>{/if}

    {#if summary}
      <p class="summary">
        Imported {summary.imported} transaction{summary.imported === 1 ? "" : "s"}.
        {#if summary.skipped_duplicates > 0}
          Skipped {summary.skipped_duplicates} already-imported duplicate{summary.skipped_duplicates ===
          1
            ? ""
            : "s"}.
        {/if}
      </p>
      <button type="button" onclick={onClose}>Close</button>
    {:else if !preview}
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
        one use the fallback chosen below, or "Other" if you leave that unset.
      </p>

      <div class="targets">
        <select bind:value={selectedCategoryId}>
          <option value="">Fallback category (optional)…</option>
          {#each categoryOptions as c (c.id)}
            <option value={c.id}>{c.label}</option>
          {/each}
        </select>
        <select bind:value={selectedAccountId}>
          <option value="" disabled selected>Destination account…</option>
          {#each activeAccounts as a (a.id)}
            <option value={a.id}>{a.name}</option>
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
                    ? formatMinorUnits(row.amount_minor_units)
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
          disabled={importing || includableCount === 0 || !selectedAccountId}
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
    background: var(--dialog-bg, #f6f6f6);
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

  .error {
    color: #d33;
  }

  .hint {
    opacity: 0.8;
    font-size: 0.9rem;
  }

  .summary {
    font-weight: 600;
  }

  .targets {
    display: flex;
    gap: 0.5rem;
  }

  select,
  input[type="file"] {
    border-radius: 6px;
    border: 1px solid rgba(0, 0, 0, 0.15);
    padding: 0.4rem 0.6rem;
    font-family: inherit;
  }

  .dropzone {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 1.25rem;
    border: 2px dashed rgba(0, 0, 0, 0.2);
    border-radius: 10px;
    transition: border-color 0.15s, background-color 0.15s;
  }

  .dropzone.drag-over {
    border-color: #396cd8;
    background-color: rgba(57, 108, 216, 0.08);
  }

  .dropzone-hint {
    margin: 0;
    font-size: 0.85rem;
    opacity: 0.7;
  }

  .rows {
    max-height: 20rem;
    overflow-y: auto;
    border: 1px solid rgba(0, 0, 0, 0.1);
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
    border-bottom: 1px solid rgba(0, 0, 0, 0.06);
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
    background-color: #396cd8;
    color: white;
    font-size: 0.9rem;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  @media (prefers-color-scheme: dark) {
    .dialog {
      background: #2f2f2f;
    }

    select,
    input[type="file"] {
      background-color: rgba(255, 255, 255, 0.06);
      border-color: rgba(255, 255, 255, 0.15);
      color: inherit;
    }

    .rows {
      border-color: rgba(255, 255, 255, 0.1);
    }

    tbody tr:not(:last-child) {
      border-bottom-color: rgba(255, 255, 255, 0.08);
    }

    .dropzone {
      border-color: rgba(255, 255, 255, 0.2);
    }

    .dropzone.drag-over {
      border-color: #6f9bf0;
      background-color: rgba(111, 155, 240, 0.12);
    }
  }
</style>
