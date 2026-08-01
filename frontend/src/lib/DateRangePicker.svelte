<script lang="ts">
  import { Calendar, ChevronLeft, ChevronRight } from "@lucide/svelte";

  type Mode = "day" | "month";
  type Ymd = { y: number; m: number; d: number };

  let {
    start,
    end,
    onChange,
  }: {
    start: string;
    end: string;
    onChange: (start: string, end: string) => void;
  } = $props();

  const MONTH_LABELS = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
  ];
  const MONTH_LABELS_FULL = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
  ];
  const WEEKDAY_LABELS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

  function pad(n: number): string {
    return String(n).padStart(2, "0");
  }

  function parseIso(iso: string): Ymd {
    const [y, m, d] = iso.split("-").map(Number);
    return { y, m: m - 1, d };
  }

  function toIso({ y, m, d }: Ymd): string {
    return `${y}-${pad(m + 1)}-${pad(d)}`;
  }

  function daysInMonth(y: number, m: number): number {
    return new Date(y, m + 1, 0).getDate();
  }

  function cmpDay(a: Ymd, b: Ymd): number {
    return a.y - b.y || a.m - b.m || a.d - b.d;
  }

  function cmpMonth(a: Ymd, b: Ymd): number {
    return a.y - b.y || a.m - b.m;
  }

  function formatShort(iso: string): string {
    const { y, m, d } = parseIso(iso);
    return `${d} ${MONTH_LABELS[m]} ${y}`;
  }

  const today = new Date();
  const todayYmd: Ymd = {
    y: today.getFullYear(),
    m: today.getMonth(),
    d: today.getDate(),
  };

  let open = $state(false);
  let mode = $state<Mode>("day");
  let containerEl: HTMLDivElement | undefined = $state();

  let viewYear = $state(todayYmd.y);
  let viewMonth = $state(todayYmd.m);

  // The one end of a range the user has clicked so far — cleared once its
  // partner click completes the range (or the picker is reopened/closed).
  let pendingStart = $state<Ymd | null>(null);
  let hovered = $state<Ymd | null>(null);

  let selectedStart = $derived(parseIso(start));
  let selectedEnd = $derived(parseIso(end));

  function openPicker() {
    const anchor = parseIso(end || start);
    viewYear = anchor.y;
    viewMonth = anchor.m;
    pendingStart = null;
    hovered = null;
    open = true;
  }

  function toggle() {
    if (open) open = false;
    else openPicker();
  }

  function handleWindowClick(event: MouseEvent) {
    if (open && containerEl && !containerEl.contains(event.target as Node)) {
      open = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      open = false;
    }
  }

  function switchMode(next: Mode) {
    mode = next;
    pendingStart = null;
    hovered = null;
  }

  function prevPeriod() {
    if (mode === "month") {
      viewYear -= 1;
    } else if (viewMonth === 0) {
      viewMonth = 11;
      viewYear -= 1;
    } else {
      viewMonth -= 1;
    }
  }

  function nextPeriod() {
    if (mode === "month") {
      viewYear += 1;
    } else if (viewMonth === 11) {
      viewMonth = 0;
      viewYear += 1;
    } else {
      viewMonth += 1;
    }
  }

  /** Completes a range from two clicked endpoints, in whichever order they
   * were clicked. In month mode the range widens to whole calendar months —
   * first day of the earlier month through the last day of the later one. */
  function commitRange(a: Ymd, b: Ymd, wholeMonth: boolean) {
    const [lo, hi] = cmpDay(a, b) <= 0 ? [a, b] : [b, a];
    const startYmd = wholeMonth ? { ...lo, d: 1 } : lo;
    const endYmd = wholeMonth ? { ...hi, d: daysInMonth(hi.y, hi.m) } : hi;
    onChange(toIso(startYmd), toIso(endYmd));
    pendingStart = null;
    hovered = null;
    open = false;
  }

  function pickDay(y: number, m: number, d: number) {
    const clicked = { y, m, d };
    if (!pendingStart) pendingStart = clicked;
    else commitRange(pendingStart, clicked, false);
  }

  function pickMonth(y: number, m: number) {
    const clicked = { y, m, d: 1 };
    if (!pendingStart) pendingStart = clicked;
    else commitRange(pendingStart, clicked, true);
  }

  type DayCell = { y: number; m: number; d: number; inMonth: boolean };

  let dayCells = $derived.by((): DayCell[] => {
    const first = new Date(viewYear, viewMonth, 1);
    const firstWeekday = (first.getDay() + 6) % 7; // 0 = Monday
    const total = daysInMonth(viewYear, viewMonth);
    const prevMonth = viewMonth === 0 ? 11 : viewMonth - 1;
    const prevYear = viewMonth === 0 ? viewYear - 1 : viewYear;
    const prevTotal = daysInMonth(prevYear, prevMonth);
    const nextMonth = viewMonth === 11 ? 0 : viewMonth + 1;
    const nextYear = viewMonth === 11 ? viewYear + 1 : viewYear;

    const cells: DayCell[] = [];
    for (let i = firstWeekday - 1; i >= 0; i--) {
      cells.push({ y: prevYear, m: prevMonth, d: prevTotal - i, inMonth: false });
    }
    for (let d = 1; d <= total; d++) {
      cells.push({ y: viewYear, m: viewMonth, d, inMonth: true });
    }
    let nd = 1;
    while (cells.length < 42) {
      cells.push({ y: nextYear, m: nextMonth, d: nd++, inMonth: false });
    }
    return cells;
  });

  function isSameDay(a: Ymd, b: Ymd): boolean {
    return cmpDay(a, b) === 0;
  }

  function isSameMonth(a: Ymd, b: Ymd): boolean {
    return cmpMonth(a, b) === 0;
  }

  function dayInRange(cell: Ymd): boolean {
    const rangeStart = pendingStart ?? selectedStart;
    const rangeEnd = pendingStart ? (hovered ?? pendingStart) : selectedEnd;
    const [lo, hi] =
      cmpDay(rangeStart, rangeEnd) <= 0 ? [rangeStart, rangeEnd] : [rangeEnd, rangeStart];
    return cmpDay(cell, lo) >= 0 && cmpDay(cell, hi) <= 0;
  }

  function monthInRange(cell: Ymd): boolean {
    const rangeStart = pendingStart ?? selectedStart;
    const rangeEnd = pendingStart ? (hovered ?? pendingStart) : selectedEnd;
    const [lo, hi] =
      cmpMonth(rangeStart, rangeEnd) <= 0 ? [rangeStart, rangeEnd] : [rangeEnd, rangeStart];
    return cmpMonth(cell, lo) >= 0 && cmpMonth(cell, hi) <= 0;
  }

  // A pending click starts a fresh pick and replaces the old committed range
  // in the display entirely — otherwise the old start, the old end, and the
  // new pending click would all show as endpoints at once.
  function isDayEndpoint(cell: Ymd): boolean {
    if (pendingStart) return isSameDay(cell, pendingStart);
    return isSameDay(cell, selectedStart) || isSameDay(cell, selectedEnd);
  }

  function isMonthEndpoint(cell: Ymd): boolean {
    if (pendingStart) return isSameMonth(cell, pendingStart);
    return isSameMonth(cell, selectedStart) || isSameMonth(cell, selectedEnd);
  }
</script>

<svelte:window onclick={handleWindowClick} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="date-range-picker" bind:this={containerEl} onkeydown={handleKeydown}>
  <button type="button" class="trigger" class:active={open} onclick={toggle}>
    <Calendar size={14} />
    <span>{formatShort(start)} – {formatShort(end)}</span>
  </button>

  {#if open}
    <div class="dropdown">
      <div class="mode-tabs">
        <button
          type="button"
          class:active={mode === "day"}
          onclick={() => switchMode("day")}>By date</button
        >
        <button
          type="button"
          class:active={mode === "month"}
          onclick={() => switchMode("month")}>By month</button
        >
      </div>

      <div class="nav">
        <button
          type="button"
          class="nav-button"
          onclick={prevPeriod}
          aria-label="Previous"
        >
          <ChevronLeft size={16} />
        </button>
        <span class="nav-label">
          {mode === "month" ? viewYear : `${MONTH_LABELS_FULL[viewMonth]} ${viewYear}`}
        </span>
        <button
          type="button"
          class="nav-button"
          onclick={nextPeriod}
          aria-label="Next"
        >
          <ChevronRight size={16} />
        </button>
      </div>

      {#if mode === "day"}
        <div class="weekdays">
          {#each WEEKDAY_LABELS as label (label)}
            <span>{label}</span>
          {/each}
        </div>
        <div class="day-grid">
          {#each dayCells as cell (`${cell.y}-${cell.m}-${cell.d}-${cell.inMonth}`)}
            <button
              type="button"
              class="day-cell"
              class:dim={!cell.inMonth}
              class:today={isSameDay(cell, todayYmd)}
              class:endpoint={isDayEndpoint(cell)}
              class:in-range={dayInRange(cell)}
              onmouseenter={() => (hovered = cell)}
              onclick={() => pickDay(cell.y, cell.m, cell.d)}
            >
              {cell.d}
            </button>
          {/each}
        </div>
      {:else}
        <div class="month-grid">
          {#each MONTH_LABELS as label, m (label)}
            {@const cell = { y: viewYear, m, d: 1 }}
            <button
              type="button"
              class="month-cell"
              class:today={viewYear === todayYmd.y && m === todayYmd.m}
              class:endpoint={isMonthEndpoint(cell)}
              class:in-range={monthInRange(cell)}
              onmouseenter={() => (hovered = cell)}
              onclick={() => pickMonth(viewYear, m)}
            >
              {label}
            </button>
          {/each}
        </div>
      {/if}

      <p class="hint">
        {#if pendingStart}
          {mode === "month" ? "Pick the end month…" : "Pick the end date…"}
        {:else}
          Click a start, then an end to set the range.
        {/if}
      </p>
    </div>
  {/if}
</div>

<style>
  .date-range-picker {
    position: relative;
    display: inline-flex;
  }

  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    border-radius: 6px;
    border: 1px solid var(--color-shade-3);
    background-color: var(--color-shade-2);
    color: inherit;
    padding: 0.4rem 0.7rem;
    font-size: 0.85rem;
    font-family: inherit;
    cursor: pointer;
    white-space: nowrap;
  }

  .trigger.active {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  .dropdown {
    position: absolute;
    top: calc(100% + 0.25rem);
    left: 0;
    z-index: 100;
    width: 17.5rem;
    background: var(--color-shade-2);
    border: 1px solid var(--color-shade-3);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    padding: 0.6rem;
  }

  .mode-tabs {
    display: flex;
    gap: 0.3rem;
    margin-bottom: 0.6rem;
  }

  .mode-tabs button {
    flex: 1;
    border: 1px solid var(--color-shade-3);
    border-radius: 6px;
    background: transparent;
    color: inherit;
    padding: 0.3rem 0;
    font-size: 0.78rem;
    font-family: inherit;
    cursor: pointer;
  }

  .mode-tabs button.active {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
    border-color: var(--color-accent);
  }

  .nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.4rem;
  }

  .nav-label {
    font-size: 0.85rem;
    font-weight: 600;
  }

  .nav-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    color: inherit;
    padding: 0.2rem;
    border-radius: 4px;
    cursor: pointer;
  }

  .nav-button:hover {
    background-color: var(--color-shade-3);
  }

  .weekdays {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    text-align: center;
    font-size: 0.7rem;
    opacity: 0.6;
    margin-bottom: 0.2rem;
  }

  .day-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    row-gap: 0.15rem;
  }

  .day-cell {
    border: none;
    background: transparent;
    color: inherit;
    font-size: 0.8rem;
    font-family: inherit;
    padding: 0.35rem 0;
    cursor: pointer;
    border-radius: 999px;
  }

  .day-cell.dim {
    opacity: 0.35;
  }

  .day-cell.today {
    box-shadow: inset 0 0 0 1px var(--color-accent);
  }

  /* A contiguous range reads as one bar, not a row of separate pills — flat
     edges between adjacent in-range cells, rounded only at the two ends. */
  .day-cell.in-range {
    background-color: var(--color-shade-3);
    border-radius: 0;
  }

  .day-cell.in-range:nth-child(7n + 1) {
    border-top-left-radius: 999px;
    border-bottom-left-radius: 999px;
  }

  .day-cell.in-range:nth-child(7n) {
    border-top-right-radius: 999px;
    border-bottom-right-radius: 999px;
  }

  .day-cell.endpoint {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
    border-radius: 999px;
  }

  .month-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.3rem;
  }

  .month-cell {
    border: none;
    background: transparent;
    color: inherit;
    font-size: 0.8rem;
    font-family: inherit;
    padding: 0.5rem 0;
    cursor: pointer;
    border-radius: 6px;
  }

  .month-cell.today {
    box-shadow: inset 0 0 0 1px var(--color-accent);
  }

  .month-cell.in-range {
    background-color: var(--color-shade-3);
    border-radius: 0;
  }

  .month-cell.in-range:nth-child(3n + 1) {
    border-top-left-radius: 6px;
    border-bottom-left-radius: 6px;
  }

  .month-cell.in-range:nth-child(3n) {
    border-top-right-radius: 6px;
    border-bottom-right-radius: 6px;
  }

  .month-cell.endpoint {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
    border-radius: 6px;
  }

  .hint {
    margin: 0.5rem 0 0;
    font-size: 0.72rem;
    opacity: 0.6;
    text-align: center;
  }
</style>
