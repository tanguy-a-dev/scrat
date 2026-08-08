<script lang="ts">
  import { onMount } from "svelte";
  import { message } from "@tauri-apps/plugin-dialog";
  import { toast } from "$lib/toasts.svelte";
  import { describeError, t, tp } from "$lib/i18n.svelte";
  import Checkbox from "$lib/Checkbox.svelte";
  import SearchSelect from "$lib/SearchSelect.svelte";
  import { LoaderCircle, Pencil, Tags } from "@lucide/svelte";
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
  /** The preview table renders this many rows at a time instead of the whole
   * file at once — a multi-thousand-row export would otherwise mean that
   * many checkboxes and table rows in the DOM on first paint. Grows by
   * `ROWS_PAGE_SIZE` as the user scrolls `.rows` toward its bottom; every
   * selection/count elsewhere in this dialog still operates on the full
   * `preview.rows`, only what's painted is paginated. */
  let visibleRowCount = $state(0);
  const ROWS_PAGE_SIZE = 20;
  let visibleRows = $derived(preview?.rows.slice(0, visibleRowCount) ?? []);

  function handleRowsScroll(event: Event) {
    if (!preview) return;
    const el = event.currentTarget as HTMLDivElement;
    const nearBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 64;
    if (nearBottom && visibleRowCount < preview.rows.length) {
      visibleRowCount = Math.min(visibleRowCount + ROWS_PAGE_SIZE, preview.rows.length);
    }
  }
  /** Kept so a corrected mapping can be applied to the same file without
   * asking the user to pick it again. */
  let fileBytes = $state<number[] | null>(null);
  let included = $state<boolean[]>([]);
  /** Parallel to `preview.rows` — true when a row has the same date, amount,
   * and description as a transaction already sitting in the destination
   * account. A hint, not a constraint: nothing stops a flagged row from
   * being ticked back on, the same way the ledger itself never rejects a
   * duplicate write (see `TransactionFingerprint`). */
  let duplicateFlags = $state<boolean[]>([]);
  let selectedCategoryId = $state("");
  let selectedAccountId = $state("");
  let prioritizeHistoricalCategory = $state(false);
  let detectCategoryFromHistory = $state(true);
  let importing = $state(false);
  let dragOver = $state(false);
  let mappingOpen = $state(false);
  let categoriesOpen = $state(false);
  let remapping = $state(false);

  let categoryOptions = $derived(buildCategoryOptions(categories));
  /** An explicit "unset" entry, the same pattern the transactions list uses
   * for its category filter — without one, there'd be no way to navigate
   * the searchable dropdown back to "no fallback" once something is picked. */
  let fallbackCategoryOptions = $derived([
    { id: "", label: t("import.uncategorizedDefault") },
    ...categoryOptions,
  ]);
  let accountOptions = $derived([
    { id: "", label: t("import.defaultAccount") },
    ...accounts.map((a) => ({
      id: a.id,
      label: a.is_default ? t("import.accountIsDefault", { name: a.name }) : a.name,
    })),
  ]);

  let includableCount = $derived(
    preview?.rows.filter((r, i) => included[i] && r.date && r.amount_minor_units).length ?? 0,
  );

  /** Rows that can't be imported at all can't be ticked either, so they're
   * excluded from every count the header checkbox reasons about — otherwise
   * "select all" could never reach "all". */
  let selectableIndexes = $derived(
    (preview?.rows ?? [])
      .map((r, i) => (r.date !== null && r.amount_minor_units !== null ? i : -1))
      .filter((i) => i >= 0),
  );
  let allSelected = $derived(
    selectableIndexes.length > 0 && selectableIndexes.every((i) => included[i]),
  );
  let anySelected = $derived(selectableIndexes.some((i) => included[i]));

  function toggleSelectAll() {
    const next = !allSelected;
    for (const i of selectableIndexes) included[i] = next;
  }

  /** Shift-click extends from the last row clicked, matching the
   * transactions list. */
  let lastClickedIndex = $state<number | null>(null);

  function handleRowPress(index: number, event: MouseEvent) {
    if (event.shiftKey && lastClickedIndex !== null) {
      const [from, to] = [lastClickedIndex, index].sort((a, b) => a - b);
      const value = !included[index];
      for (let i = from; i <= to; i++) {
        if (selectableIndexes.includes(i)) included[i] = value;
      }
    } else {
      included[index] = !included[index];
    }
    lastClickedIndex = index;
    if (event.shiftKey) return; // a discrete range-select, not a drag
    dragging = true;
    dragPaintValue = included[index];
  }

  // Click-and-drag multi-select, same as the transactions list: press on a
  // checkbox and sweep across rows while the button stays down.
  let dragging = $state(false);
  let dragPaintValue = $state(false);

  function continueDrag(index: number) {
    if (!dragging || !selectableIndexes.includes(index)) return;
    included[index] = dragPaintValue;
    lastClickedIndex = index;
  }

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
    const name = column.header || t("import.columnLabel", { number: column.index + 1 });
    const samples = column.samples.slice(0, 2).join(", ");
    return samples ? t("import.columnWithSamples", { number: column.index + 1, name, samples }) : t("import.columnWithName", { number: column.index + 1, name });
  }

  /** Above this, a file is almost certainly not a bank's CSV export — a
   * genuine one is a few thousand rows at most. Checked here, before the
   * bytes ever cross the IPC bridge, so a huge file doesn't get serialized
   * into a giant number array just to be rejected on the other side. The
   * Rust command re-checks the same limit in case this dialog isn't the
   * only caller someday. */
  const MAX_CSV_FILE_BYTES = 20 * 1024 * 1024;

  /** Bumped on every call so a slow response from a superseded check (an
   * account switched again, or a new file/mapping loaded, before the first
   * one returned) can't clobber a newer result that already landed. */
  let duplicateCheckToken = 0;

  /** Re-flags which rows collide with a transaction already on the
   * destination account, and re-applies the "uncheck duplicates by default"
   * rule on top of whatever `include_by_default` already decided. Run after
   * every (re)parse of the file and whenever the destination account
   * changes, since a duplicate is only a duplicate on the account it
   * collides with. */
  async function refreshDuplicateFlags() {
    if (!preview) return;
    const rows = preview.rows;
    const token = ++duplicateCheckToken;
    const candidates = rows
      .map((r, i) => ({ i, r }))
      .filter(({ r }) => r.date !== null && r.amount_minor_units !== null);
    if (candidates.length === 0) {
      duplicateFlags = rows.map(() => false);
      return;
    }
    try {
      const flags = await api.checkDuplicateTransactions(
        selectedAccountId || null,
        candidates.map(({ r }) => ({
          date: r.date as string,
          amount_minor_units: r.amount_minor_units as number,
          description: r.description,
        })),
      );
      if (token !== duplicateCheckToken) return;
      const next = rows.map(() => false);
      candidates.forEach(({ i }, idx) => {
        next[i] = flags[idx] ?? false;
      });
      duplicateFlags = next;
      included = rows.map((r, i) => r.include_by_default && !next[i]);
    } catch {
      // Best-effort: the row selection detection already set stands.
    }
  }

  function handleAccountChange(id: string) {
    selectedAccountId = id;
    refreshDuplicateFlags();
  }

  async function loadBytes(bytes: number[]) {
    if (bytes.length > MAX_CSV_FILE_BYTES) {
      await message(
        t("import.fileTooLarge", {
          size: (bytes.length / (1024 * 1024)).toFixed(1),
          limit: MAX_CSV_FILE_BYTES / (1024 * 1024),
        }),
        { title: t("import.dialogTitle"), kind: "error" },
      );
      return;
    }
    preview = null;
    fileBytes = bytes;
    try {
      const result = await api.previewCsvImport(bytes);
      preview = result;
      visibleRowCount = Math.min(ROWS_PAGE_SIZE, result.rows.length);
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
      await refreshDuplicateFlags();
    } catch (e) {
      await message(describeError(e), { title: t("import.dialogTitle"), kind: "error" });
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
      visibleRowCount = Math.min(ROWS_PAGE_SIZE, result.rows.length);
      included = result.rows.map((r) => r.include_by_default);
      await refreshDuplicateFlags();
    } catch (e) {
      await message(describeError(e), { title: t("import.dialogTitle"), kind: "error" });
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

  /** A file carries exactly two facts about an amount — where money out is
   * written and where money in is — so the editor asks those two questions
   * and nothing else. A single signed column is the case where both answers
   * are the same column, not a third layout the user has to classify the
   * file into first. */
  let debitColumn = $derived(
    preview?.mapping.amount == null
      ? null
      : preview.mapping.amount.kind === "single"
        ? preview.mapping.amount.column
        : preview.mapping.amount.debit,
  );
  let creditColumn = $derived(
    preview?.mapping.amount == null
      ? null
      : preview.mapping.amount.kind === "single"
        ? preview.mapping.amount.column
        : preview.mapping.amount.credit,
  );

  function setAmountColumns(debit: number | null, credit: number | null) {
    // Half an answer can't be read: taking the one side that is set would
    // import money moving in one direction only, which is the failure the
    // debit/credit pair exists to prevent. Clearing either side clears both.
    if (debit === null || credit === null) {
      updateMapping({ amount: null });
    } else if (debit === credit) {
      updateMapping({ amount: { kind: "single", column: debit } });
    } else {
      updateMapping({ amount: { kind: "debit_credit", debit, credit } });
    }
  }

  /** Answering one side while the other is unset assumes the common shape —
   * one signed column — so a preview appears immediately; picking a
   * different column on the other side then splits the pair. */
  function setDebitColumn(value: string) {
    const column = toColumn(value);
    setAmountColumns(column, column === null ? null : (creditColumn ?? column));
  }

  function setCreditColumn(value: string) {
    const column = toColumn(value);
    setAmountColumns(column === null ? null : (debitColumn ?? column), column);
  }

  function toggleHasHeader() {
    if (!preview) return;
    updateMapping({ has_header: !preview.mapping.has_header });
  }

  let descriptionColumns = $derived(preview?.mapping.description_columns ?? []);

  function toggleDescriptionColumn(index: number, checked: boolean) {
    const next = checked
      ? [...descriptionColumns, index].sort((a, b) => a - b)
      : descriptionColumns.filter((c) => c !== index);
    updateMapping({ description_columns: next });
  }

  async function loadFile(file: File) {
    // Checked against `file.size` before reading, so an oversized file never
    // gets pulled into memory as an array buffer just to be rejected below.
    if (file.size > MAX_CSV_FILE_BYTES) {
      await message(
        t("import.fileTooLarge", {
          size: (file.size / (1024 * 1024)).toFixed(1),
          limit: MAX_CSV_FILE_BYTES / (1024 * 1024),
        }),
        { title: t("import.dialogTitle"), kind: "error" },
      );
      return;
    }
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
        prioritizeHistoricalCategory,
        detectCategoryFromHistory,
      );
      onImported();
      onClose();
      // Mirrored rows wrote entries on an account the user didn't import
      // into, so its balance just changed without them asking. Say it
      // plainly rather than let them find it later and wonder.
      if (summary.mirrored > 0) {
        toast.success(
          t("import.mirroredSummary", {
            imported: summary.imported,
            mirrored: summary.mirrored,
          }),
        );
      }
    } catch (e) {
      await message(describeError(e), { title: t("import.dialogTitle"), kind: "error" });
    } finally {
      importing = false;
    }
  }
</script>

<!-- Catches the drag's mouseup wherever it lands, including outside any row. -->
<svelte:window onmouseup={() => (dragging = false)} />

<div class="backdrop">
  <div class="dialog">
    <h2>{t("import.title")}</h2>

    {#if !preview}
      <div
        class="dropzone"
        class:drag-over={dragOver}
        role="group"
        aria-label={t("import.dropzone")}
        ondragover={handleDragOver}
        ondragleave={handleDragLeave}
        ondrop={handleDrop}
      >
        <input type="file" accept=".csv,text/csv" onchange={handleFileChange} />
        <p class="dropzone-hint">{t("import.dropHint")}</p>
      </div>
      <div class="actions">
        <button type="button" onclick={onClose}>{t("common.cancel")}</button>
      </div>
    {:else}
      {#if mappingLooksWrong}
        <p class="warning">
          {t("import.mappingLooksWrong", {
            dates: Math.round(preview.date_confidence * 100),
            amounts: Math.round(preview.amount_confidence * 100),
          })}
        </p>
      {/if}

      <details class="mapping" bind:open={mappingOpen}>
        <summary>
          <Pencil size={13} aria-hidden="true" />
          <span>{t("import.editColumns")}</span>
          {#if preview.remembered}
            <span class="badge" title={t("import.savedBadgeTitle")}
              >{t("import.savedBadge")}</span
            >
          {/if}
          {#if remapping}<span class="badge muted">{t("import.rereading")}</span>{/if}
        </summary>

        <div class="mapping-grid">
          <label>
            {t("common.date")}
            <select
              value={preview.mapping.date_column ?? ""}
              onchange={(e) =>
                updateMapping({ date_column: toColumn(e.currentTarget.value) })}
            >
              <option value="">{t("import.notSet")}</option>
              {#each preview.columns as c (c.index)}
                <option value={c.index}>{columnLabel(c)}</option>
              {/each}
            </select>
          </label>

          <label>
            {t("import.dateFormat")}
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
            {t("import.moneyOut")}
            <select
              value={debitColumn ?? ""}
              onchange={(e) => setDebitColumn(e.currentTarget.value)}
            >
              <option value="">{t("import.notSet")}</option>
              {#each preview.columns as c (c.index)}
                <option value={c.index}>{columnLabel(c)}</option>
              {/each}
            </select>
          </label>

          <label>
            {t("import.moneyIn")}
            <select
              value={creditColumn ?? ""}
              onchange={(e) => setCreditColumn(e.currentTarget.value)}
            >
              <option value="">{t("import.notSet")}</option>
              {#each preview.columns as c (c.index)}
                <option value={c.index}>{columnLabel(c)}</option>
              {/each}
            </select>
            <span class="field-hint">{t("import.moneyInHint")}</span>
          </label>

          <label>
            {t("transactions.type")}
            <select
              value={preview.mapping.operation_kind_column ?? ""}
              onchange={(e) =>
                updateMapping({ operation_kind_column: toColumn(e.currentTarget.value) })}
            >
              <option value="">{t("import.readFromDescription")}</option>
              {#each preview.columns as c (c.index)}
                <option value={c.index}>{columnLabel(c)}</option>
              {/each}
            </select>
          </label>

          <label>
            {t("common.category")}
            <select
              value={preview.mapping.category_column ?? ""}
              onchange={(e) =>
                updateMapping({ category_column: toColumn(e.currentTarget.value) })}
            >
              <option value="">{t("common.none")}</option>
              {#each preview.columns as c (c.index)}
                <option value={c.index}>{columnLabel(c)}</option>
              {/each}
            </select>
          </label>

          <label>
            {t("common.subcategory")}
            <select
              value={preview.mapping.subcategory_column ?? ""}
              onchange={(e) =>
                updateMapping({ subcategory_column: toColumn(e.currentTarget.value) })}
            >
              <option value="">{t("common.none")}</option>
              {#each preview.columns as c (c.index)}
                <option value={c.index}>{columnLabel(c)}</option>
              {/each}
            </select>
          </label>
        </div>

        <fieldset class="description-source">
          <legend>{t("common.description")}</legend>
          <p class="description-hint">
            {#if descriptionColumns.length === 0}
              Pick at least one column — without it every row imports with a
              blank description.
            {:else}
              Joined in this order.
            {/if}
          </p>
          <div class="description-columns">
            {#each preview.columns as c (c.index)}
              <span class="inline">
                <Checkbox
                  size="sm"
                  checked={descriptionColumns.includes(c.index)}
                  ariaLabel={`Use ${columnLabel(c)} in the description`}
                  onpress={() =>
                    toggleDescriptionColumn(c.index, !descriptionColumns.includes(c.index))}
                />
                {columnLabel(c)}
              </span>
            {/each}
          </div>
        </fieldset>

        <span class="inline">
          <Checkbox
            size="sm"
            checked={preview.mapping.has_header}
            ariaLabel={t("import.firstRowHeader")}
            onpress={toggleHasHeader}
          />
          {t("import.firstRowHeader")}
        </span>
      </details>

      <details class="mapping" bind:open={categoriesOpen}>
        <summary>
          <Tags size={13} aria-hidden="true" />
          <span>{t("import.categoriesSettings")}</span>
        </summary>

        <label class="default-category">
          <span class="caption">{t("import.defaultCategory")}</span>
          <SearchSelect
            options={fallbackCategoryOptions}
            value={selectedCategoryId}
            onChange={(id) => (selectedCategoryId = id)}
            placeholder={t("import.uncategorizedDefaultPlaceholder")}
            searchPlaceholder={t("categories.searchCategory")}
          />
        </label>

        <span class="inline">
          <Checkbox
            size="sm"
            checked={detectCategoryFromHistory}
            ariaLabel={t("import.reusePastCategories")}
            onpress={() => (detectCategoryFromHistory = !detectCategoryFromHistory)}
          />
          {t("import.reusePastCategories")}
        </span>

        <span class="inline">
          <Checkbox
            size="sm"
            checked={prioritizeHistoricalCategory}
            disabled={!detectCategoryFromHistory}
            ariaLabel={t("import.pastCategoriesOverride")}
            onpress={() => (prioritizeHistoricalCategory = !prioritizeHistoricalCategory)}
          />
          {t("import.pastCategoriesOverride")}
        </span>
      </details>

      <div class="targets">
        <SearchSelect
          options={accountOptions}
          value={selectedAccountId}
          onChange={handleAccountChange}
          placeholder={t("import.destinationAccount")}
          searchPlaceholder={t("settings.searchAccount")}
        />
      </div>


      <div class="rows" onscroll={handleRowsScroll}>
        <table>
          <thead>
            <tr>
              <th class="select-header">
                <Checkbox
                  checked={allSelected}
                  indeterminate={anySelected && !allSelected}
                  ariaLabel={t("import.selectAllRows")}
                  onpress={toggleSelectAll}
                />
              </th>
              <th>{t("common.date")}</th>
              <th class="amount">{t("common.amount")}</th>
              <th>{t("common.description")}</th>
              <th>{t("transactions.type")}</th>
              <th>{t("common.category")}</th>
              <th>{t("common.subcategory")}</th>
            </tr>
          </thead>
          <tbody>
            {#each visibleRows as row, i (i)}
              {@const invalid = row.date === null || row.amount_minor_units === null}
              <tr
                class:invalid
                class:likely-balance={row.is_likely_balance_row}
                class:likely-duplicate={duplicateFlags[i] && !row.is_likely_balance_row}
                onmouseenter={() => continueDrag(i)}
              >
                <td class="select-cell">
                  <Checkbox
                    checked={included[i]}
                    disabled={invalid}
                    ariaLabel={row.description
                      ? t("import.importRowWithDescription", {
                          number: i + 1,
                          description: row.description,
                        })
                      : t("import.importRow", { number: i + 1 })}
                    onpress={(event) => handleRowPress(i, event)}
                  />
                </td>
                <td class="date">{row.date ?? "—"}</td>
                <td class="amount" class:income={(row.amount_minor_units ?? 0) > 0}
                  >{row.amount_minor_units !== null
                    ? formatSignedAmount(row.amount_minor_units)
                    : "—"}</td
                >
                <td>
                  {row.description || "—"}
                  {#if row.is_likely_balance_row}
                    <span class="balance-hint">{t("import.balanceLine")}</span>
                  {/if}
                  {#if duplicateFlags[i]}
                    <span class="balance-hint"
                      >{t("import.alreadyInAccount")}</span
                    >
                  {/if}
                </td>
                <td class="suggestion">{operationKindLabel(row.operation_kind)}</td>
                <td class="suggestion">{row.csv_category ?? "—"}</td>
                <td class="suggestion">{row.csv_subcategory ?? "—"}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <div class="actions">
        <span class="actions-note">
          {t("import.rowsSelected", {
            count: includableCount,
            total: preview.rows.length,
          })}
          {#if visibleRowCount < preview.rows.length}
            {t("import.showingCount", { count: visibleRowCount })}
          {/if}
        </span>
        <button type="button" onclick={onClose}>{t("common.cancel")}</button>
        <button
          type="button"
          class="import-button"
          disabled={importing || includableCount === 0}
          onclick={handleImport}
        >
          {#if importing}
            <LoaderCircle class="spinner" size={14} aria-hidden="true" />
          {/if}
          {tp("import.importCount", includableCount)}
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
    /* Wider than a plain form dialog because the preview table now carries
       Category and Subcategory as separate columns. */
    width: min(54rem, 92vw);
    max-height: 85vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  h2 {
    margin: 0;
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
    display: flex;
    align-items: center;
    gap: 0.4rem;
    /* Reads as a clickable control, not just a disclosure label — a plain
       "Columns" heading with a browser-default triangle was easy to miss. */
    border-radius: 6px;
    padding: 0.2rem 0.35rem;
    margin: -0.2rem -0.35rem;
    transition: background-color 0.1s;
  }

  .mapping summary:hover {
    background-color: var(--color-shade-3);
  }

  .mapping summary :global(svg) {
    color: var(--color-accent);
    flex: none;
  }

  /* Says "this came from your last import" in the space a sentence would
     have taken, with the explanation on the tooltip for whoever wants it. */
  .badge {
    font-size: 0.7rem;
    font-weight: 500;
    padding: 0.05rem 0.4rem;
    border-radius: 999px;
    background-color: color-mix(in srgb, var(--color-accent) 22%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-accent) 40%, transparent);
  }

  .badge.muted {
    background: none;
    border-color: var(--color-shade-4);
    opacity: 0.7;
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

  .default-category {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.2rem;
    font-size: 0.8rem;
    margin-top: 0.6rem;
  }

  /* The dimming sits on the caption, never on the label itself: `opacity`
     below 1 makes the label a stacking context, which traps the open
     SearchSelect dropdown inside it no matter how high the dropdown's own
     z-index is. The checkbox rows below are `position: relative`, so they
     painted on top of the open dropdown — and the label's 0.9 alpha let
     their text bleed through it as well. */
  .default-category .caption {
    opacity: 0.9;
  }

  .default-category :global(.search-select) {
    max-width: 20rem;
    width: 100%;
  }

  .field-hint {
    font-size: 0.72rem;
    opacity: 0.65;
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

  .description-hint {
    margin: 0.5rem 0 0;
    font-size: 0.75rem;
    opacity: 0.65;
  }

  .description-columns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
    gap: 0.15rem 0.6rem;
    margin-top: 0.3rem;
  }

  /* SearchSelect's own root class — reached with :global since it renders
     inside a child component, outside this file's scoped styles. Widened
     past its 11rem default so a longer category path doesn't truncate. */
  .targets :global(.search-select) {
    max-width: 16rem;
    flex: 1;
  }

  .inline {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.8rem;
    margin-top: 0.35rem;
    min-width: 0;
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
    /* Seven columns can outgrow a narrow window; the table scrolls inside
       its own box rather than making the dialog scroll sideways. */
    overflow-x: auto;
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

  th {
    font-size: 0.8rem;
    white-space: nowrap;
  }

  /* Unlike the transactions list, ticking rows is the whole point of this
     dialog, so the boxes stay visible instead of appearing on hover. */
  .select-header,
  .select-cell {
    width: 1.35rem;
    padding-right: 0.25rem;
  }

  td.date {
    white-space: nowrap;
  }

  td.amount,
  th.amount {
    text-align: right;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  /* Money in and money out are the one distinction worth colour here: a
     debit/credit file mapped the wrong way round shows up instantly. */
  td.amount.income {
    color: var(--color-accent);
  }

  tr.invalid {
    opacity: 0.5;
  }

  tr.likely-balance:not(.invalid) {
    background-color: color-mix(in srgb, var(--color-accent) 8%, transparent);
  }

  tr.likely-duplicate:not(.invalid) {
    background-color: color-mix(in srgb, var(--color-shade-4) 25%, transparent);
  }

  .balance-hint {
    font-style: italic;
    opacity: 0.7;
    font-size: 0.75rem;
    white-space: nowrap;
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
    align-items: center;
    gap: 0.5rem;
  }

  .actions-note {
    margin-right: auto;
    font-size: 0.8rem;
    opacity: 0.7;
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

  .import-button {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }

  .import-button :global(.spinner) {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
