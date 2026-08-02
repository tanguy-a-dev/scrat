<script lang="ts">
  import { onMount } from "svelte";
  import { message } from "@tauri-apps/plugin-dialog";
  import { toast } from "$lib/toasts.svelte";
  import {
    api,
    buildCategoryOptions,
    formatMoney,
    operationKindLabel,
    type AccountDto,
    type CategoryDto,
    type ColumnMappingDto,
    type ColumnSummaryDto,
    type ImportPreviewDto,
    type OperationKind,
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
  /** Kept so a corrected mapping can be applied to the same file without
   * asking the user to pick it again. */
  let fileBytes = $state<number[] | null>(null);
  let included = $state<boolean[]>([]);
  let selectedCategoryId = $state("");
  let selectedAccountId = $state("");
  let importing = $state(false);
  let dragOver = $state(false);
  let mappingOpen = $state(false);
  let remapping = $state(false);

  let categoryOptions = $derived(buildCategoryOptions(categories));

  let includableCount = $derived(
    preview?.rows.filter((r, i) => included[i] && r.date && r.amount_minor_units).length ?? 0,
  );

  /** A mapping that can't read most of the file is the case the editor
   * exists for, so say so rather than leaving the user to infer it from a
   * table of dashes. */
  let mappingLooksWrong = $derived(
    preview !== null &&
      preview.rows.length > 0 &&
      (preview.date_confidence < 0.8 || preview.amount_confidence < 0.8),
  );

  function formatSignedAmount(minorUnits: number): string {
    const sign = minorUnits < 0 ? "-" : "";
    return `${sign}${formatMoney(Math.abs(minorUnits))}`;
  }

  /** "2 · Libelle operation — PRLV PayPal Europe, VIR SEPA ACME". The samples
   * are what make a column recognizable in a headerless export, where there
   * is no name to go on at all. */
  function columnLabel(column: ColumnSummaryDto): string {
    const name = column.header || `Column ${column.index + 1}`;
    const samples = column.samples.slice(0, 2).join(", ");
    return samples ? `${column.index + 1} · ${name} — ${samples}` : `${column.index + 1} · ${name}`;
  }

  async function loadBytes(bytes: number[]) {
    preview = null;
    fileBytes = bytes;
    try {
      const result = await api.previewCsvImport(bytes);
      preview = result;
      included = result.rows.map((r) => r.include_by_default);
      // Open the editor unprompted when detection clearly failed — the user
      // shouldn't have to discover that the fix exists.
      mappingOpen =
        result.rows.length > 0 && (result.date_confidence < 0.8 || result.amount_confidence < 0.8);
      if (result.rows.length > 0) {
        const suggested = await api
          .suggestAccountForDescription(result.rows.find((r) => r.description)?.description ?? "")
          .catch(() => null);
        if (suggested) selectedAccountId = suggested;
      }
    } catch (e) {
      await message(String(e), { title: "Import CSV", kind: "error" });
    }
  }

  /** Re-reads the same file through a corrected mapping. Row selections are
   * rebuilt from the new parse: once the columns mean something different,
   * the old per-row choices no longer refer to the same values. */
  async function remap(mapping: ColumnMappingDto) {
    if (!fileBytes) return;
    remapping = true;
    try {
      const result = await api.previewCsvImport(fileBytes, mapping);
      preview = result;
      included = result.rows.map((r) => r.include_by_default);
    } catch (e) {
      await message(String(e), { title: "Import CSV", kind: "error" });
    } finally {
      remapping = false;
    }
  }

  function updateMapping(patch: Partial<ColumnMappingDto>) {
    if (!preview) return;
    remap({ ...preview.mapping, ...patch });
  }

  /** Select values are strings; "" is the null column. */
  function toColumn(value: string): number | null {
    return value === "" ? null : Number(value);
  }

  function setAmountLayout(layout: "single" | "debit_credit") {
    if (!preview) return;
    const current = preview.mapping.amount;
    if (layout === "single") {
      const column = current === null ? 0 : current.kind === "single" ? current.column : current.debit;
      updateMapping({ amount: { kind: "single", column } });
    } else {
      const debit = current === null ? 0 : current.kind === "single" ? current.column : current.debit;
      const credit =
        current !== null && current.kind === "debit_credit"
          ? current.credit
          : Math.min(debit + 1, preview.mapping.column_count - 1);
      updateMapping({ amount: { kind: "debit_credit", debit, credit } });
    }
  }

  function toggleDescriptionColumn(index: number, checked: boolean) {
    if (!preview) return;
    const current =
      preview.mapping.description.kind === "columns" ? preview.mapping.description.columns : [];
    const next = checked
      ? [...current, index].sort((a, b) => a - b)
      : current.filter((c) => c !== index);
    updateMapping({ description: { kind: "columns", columns: next } });
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
    } else if (event.key === "Enter" && preview && !importing && includableCount > 0) {
      event.preventDefault();
      handleImport();
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
        description: string;
        category: string | null;
        subcategory: string | null;
        operation_kind: OperationKind;
      }[] = [];
      preview.rows.forEach((r, i) => {
        if (included[i] && r.date !== null && r.amount_minor_units !== null) {
          rows.push({
            date: r.date,
            amount_minor_units: r.amount_minor_units,
            description: r.description,
            category: r.csv_category,
            subcategory: r.csv_subcategory,
            operation_kind: r.operation_kind,
          });
        }
      });
      const summary = await api.commitCsvImport(
        rows,
        selectedCategoryId || null,
        selectedAccountId || null,
        preview.signature,
        preview.mapping,
      );
      onImported();
      onClose();
      // Mirrored rows wrote entries on an account the user didn't import
      // into, so its balance just changed without them asking. Say it
      // plainly rather than let them find it later and wonder.
      if (summary.mirrored > 0) {
        toast.success(
          `${summary.imported} imported, ${summary.mirrored} recognized as transfers and mirrored onto the other account.`,
        );
      }
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
        Read {Math.round(preview.date_confidence * 100)}% of dates and {Math.round(
          preview.amount_confidence * 100,
        )}% of amounts. Check the rows to import — anything that looks like an
        opening/closing balance line starts unchecked. "Type" and "Category"
        come from the file itself where it has those columns; a category it
        names is created if you don't have it yet, and rows without one fall
        back to the category chosen below ("Uncategorized" if you leave it
        unset). Leave the account unset to use your default.
      </p>

      {#if mappingLooksWrong}
        <p class="warning">
          Most rows didn't come out with a usable date or amount. The columns
          were probably guessed wrong — check the mapping below.
        </p>
      {/if}

      <details class="mapping" bind:open={mappingOpen}>
        <summary>
          Columns{preview.remembered ? " — saved from your last import" : ""}{remapping
            ? " — re-reading…"
            : ""}
        </summary>

        <p class="mapping-hint">
          {#if preview.remembered}
            These are the columns you imported this bank's export with last
            time. Change any of them and the preview below re-reads the file —
            whatever you import with is what gets remembered next.
          {:else}
            Each row of the file was read using these columns. Change any of
            them and the preview below re-reads the file.
          {/if}
        </p>

        <div class="mapping-grid">
          <label>
            Date
            <select
              value={preview.mapping.date_column ?? ""}
              onchange={(e) =>
                updateMapping({ date_column: toColumn(e.currentTarget.value) })}
            >
              <option value="">Not set</option>
              {#each preview.columns as c (c.index)}
                <option value={c.index}>{columnLabel(c)}</option>
              {/each}
            </select>
          </label>

          <label>
            Date format
            <select
              value={preview.mapping.date_format}
              onchange={(e) => updateMapping({ date_format: e.currentTarget.value })}
            >
              {#each preview.date_formats as f (f.pattern)}
                <option value={f.pattern}>{f.label}</option>
              {/each}
            </select>
          </label>

          <label>
            Amount layout
            <select
              value={preview.mapping.amount?.kind ?? "single"}
              onchange={(e) =>
                setAmountLayout(e.currentTarget.value as "single" | "debit_credit")}
            >
              <option value="single">One signed column</option>
              <option value="debit_credit">Separate debit and credit columns</option>
            </select>
          </label>

          {#if preview.mapping.amount?.kind === "debit_credit"}
            {@const amount = preview.mapping.amount}
            <label>
              Debit (money out)
              <select
                value={amount.debit}
                onchange={(e) =>
                  updateMapping({
                    amount: {
                      kind: "debit_credit",
                      debit: Number(e.currentTarget.value),
                      credit: amount.credit,
                    },
                  })}
              >
                {#each preview.columns as c (c.index)}
                  <option value={c.index}>{columnLabel(c)}</option>
                {/each}
              </select>
            </label>
            <label>
              Credit (money in)
              <select
                value={amount.credit}
                onchange={(e) =>
                  updateMapping({
                    amount: {
                      kind: "debit_credit",
                      debit: amount.debit,
                      credit: Number(e.currentTarget.value),
                    },
                  })}
              >
                {#each preview.columns as c (c.index)}
                  <option value={c.index}>{columnLabel(c)}</option>
                {/each}
              </select>
            </label>
          {:else}
            <label>
              Amount
              <select
                value={preview.mapping.amount?.kind === "single"
                  ? preview.mapping.amount.column
                  : ""}
                onchange={(e) => {
                  const column = toColumn(e.currentTarget.value);
                  updateMapping({
                    amount: column === null ? null : { kind: "single", column },
                  });
                }}
              >
                <option value="">Not set</option>
                {#each preview.columns as c (c.index)}
                  <option value={c.index}>{columnLabel(c)}</option>
                {/each}
              </select>
            </label>
          {/if}

          <label>
            Type
            <select
              value={preview.mapping.operation_kind_column ?? ""}
              onchange={(e) =>
                updateMapping({ operation_kind_column: toColumn(e.currentTarget.value) })}
            >
              <option value="">Read from the description</option>
              {#each preview.columns as c (c.index)}
                <option value={c.index}>{columnLabel(c)}</option>
              {/each}
            </select>
          </label>

          <label>
            Category
            <select
              value={preview.mapping.category_column ?? ""}
              onchange={(e) =>
                updateMapping({ category_column: toColumn(e.currentTarget.value) })}
            >
              <option value="">None</option>
              {#each preview.columns as c (c.index)}
                <option value={c.index}>{columnLabel(c)}</option>
              {/each}
            </select>
          </label>

          <label>
            Subcategory
            <select
              value={preview.mapping.subcategory_column ?? ""}
              onchange={(e) =>
                updateMapping({ subcategory_column: toColumn(e.currentTarget.value) })}
            >
              <option value="">None</option>
              {#each preview.columns as c (c.index)}
                <option value={c.index}>{columnLabel(c)}</option>
              {/each}
            </select>
          </label>
        </div>

        <fieldset class="description-source">
          <legend>Description</legend>
          <label class="inline">
            <input
              type="radio"
              name="description-mode"
              checked={preview.mapping.description.kind === "remaining"}
              onchange={() => updateMapping({ description: { kind: "remaining" } })}
            />
            Every column not used above
          </label>
          <label class="inline">
            <input
              type="radio"
              name="description-mode"
              checked={preview.mapping.description.kind === "columns"}
              onchange={() => updateMapping({ description: { kind: "columns", columns: [] } })}
            />
            Only these columns
          </label>
          {#if preview.mapping.description.kind === "columns"}
            {@const chosen = preview.mapping.description.columns}
            <div class="description-columns">
              {#each preview.columns as c (c.index)}
                <label class="inline">
                  <input
                    type="checkbox"
                    checked={chosen.includes(c.index)}
                    onchange={(e) =>
                      toggleDescriptionColumn(c.index, e.currentTarget.checked)}
                  />
                  {columnLabel(c)}
                </label>
              {/each}
            </div>
          {/if}
        </fieldset>

        <label class="inline">
          <input
            type="checkbox"
            checked={preview.mapping.has_header}
            onchange={(e) => updateMapping({ has_header: e.currentTarget.checked })}
          />
          The first row is a header, not a transaction
        </label>
      </details>

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
              <th>Description</th>
              <th>Type</th>
              <th>Category</th>
            </tr>
          </thead>
          <tbody>
            {#each preview.rows as row, i (i)}
              {@const invalid = row.date === null || row.amount_minor_units === null}
              <tr class:invalid class:likely-balance={row.is_likely_balance_row}>
                <td>
                  <input
                    type="checkbox"
                    bind:checked={included[i]}
                    disabled={invalid}
                  />
                </td>
                <td class="date">{row.date ?? "—"}</td>
                <td
                  >{row.amount_minor_units !== null
                    ? formatSignedAmount(row.amount_minor_units)
                    : "—"}</td
                >
                <td>
                  {row.description || "—"}
                  {#if row.is_likely_balance_row}
                    <span class="balance-hint">(likely a balance line)</span>
                  {/if}
                </td>
                <td class="suggestion">{operationKindLabel(row.operation_kind)}</td>
                <td class="suggestion"
                  >{row.csv_subcategory
                    ? `${row.csv_category} / ${row.csv_subcategory}`
                    : row.csv_category ?? "—"}</td
                >
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

  .warning {
    margin: 0;
    padding: 0.5rem 0.7rem;
    border-radius: 8px;
    font-size: 0.85rem;
    background-color: color-mix(in srgb, var(--color-accent) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-accent) 45%, transparent);
  }

  .mapping {
    border: 1px solid var(--color-shade-3);
    border-radius: 8px;
    padding: 0.5rem 0.7rem;
  }

  .mapping summary {
    cursor: pointer;
    font-size: 0.9rem;
    font-weight: 600;
  }

  .mapping-hint {
    margin: 0.6rem 0 0;
    font-size: 0.8rem;
    opacity: 0.75;
  }

  .mapping-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
    gap: 0.5rem;
    margin-top: 0.6rem;
  }

  .mapping-grid label {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.8rem;
    opacity: 0.9;
    min-width: 0;
  }

  .mapping-grid select {
    max-width: 100%;
    font-size: 0.8rem;
  }

  .description-source {
    margin: 0.6rem 0 0;
    border: 1px solid var(--color-shade-3);
    border-radius: 8px;
    padding: 0.4rem 0.7rem 0.6rem;
  }

  .description-source legend {
    font-size: 0.8rem;
    opacity: 0.9;
    padding: 0 0.3rem;
  }

  .description-columns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
    gap: 0.15rem 0.6rem;
    margin-top: 0.3rem;
  }

  label.inline {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.8rem;
    margin-top: 0.35rem;
    min-width: 0;
  }

  label.inline input {
    flex: none;
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
    /* The dialog is a flex column, and a scrollable child is free to shrink
       below its content — expanding the mapping editor above squashed this
       to a couple of pixels. The dialog itself scrolls; this must not. */
    flex: none;
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

  td.date {
    white-space: nowrap;
  }

  tr.invalid {
    opacity: 0.5;
  }

  tr.likely-balance:not(.invalid) {
    background-color: color-mix(in srgb, var(--color-accent) 8%, transparent);
  }

  .balance-hint {
    font-style: italic;
    opacity: 0.7;
    font-size: 0.8rem;
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
