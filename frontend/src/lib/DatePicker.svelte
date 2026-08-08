<script lang="ts">
  /** Single-date equivalent of `DateRangePicker` — same themed calendar
   * dropdown, but one click picks one day and closes instead of waiting on
   * a second click to complete a range. Kept separate from
   * `DateRangePicker` rather than generalizing that one: a range picker
   * threading a "single value" mode through its pending-start/hover-range
   * logic would obscure both cases for the sake of one shared file. */
  import { Calendar, ChevronLeft, ChevronRight } from "@lucide/svelte";
  import { monthNames, shortMonthNames, t, weekdayLabels } from "$lib/i18n.svelte";

  type Ymd = { y: number; m: number; d: number };

  let {
    value,
    onChange,
  }: {
    value: string;
    onChange: (date: string) => void;
  } = $props();

  // Locale-aware, and read through `$derived` so a language change
  // repaints an open calendar rather than leaving it in the old one.
  const MONTH_LABELS = $derived(shortMonthNames());
  const MONTH_LABELS_FULL = $derived(monthNames());
  const WEEKDAY_LABELS = $derived(weekdayLabels());

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

  function isSameDay(a: Ymd, b: Ymd): boolean {
    return a.y === b.y && a.m === b.m && a.d === b.d;
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
  let containerEl: HTMLDivElement | undefined = $state();

  let selected = $derived(parseIso(value));
  let viewYear = $state(todayYmd.y);
  let viewMonth = $state(todayYmd.m);

  function openPicker() {
    const anchor = parseIso(value);
    viewYear = anchor.y;
    viewMonth = anchor.m;
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

  function prevMonth() {
    if (viewMonth === 0) {
      viewMonth = 11;
      viewYear -= 1;
    } else {
      viewMonth -= 1;
    }
  }

  function nextMonth() {
    if (viewMonth === 11) {
      viewMonth = 0;
      viewYear += 1;
    } else {
      viewMonth += 1;
    }
  }

  function pickDay(y: number, m: number, d: number) {
    onChange(toIso({ y, m, d }));
    open = false;
  }

  type DayCell = { y: number; m: number; d: number; inMonth: boolean };

  let dayCells = $derived.by((): DayCell[] => {
    const first = new Date(viewYear, viewMonth, 1);
    const firstWeekday = (first.getDay() + 6) % 7; // 0 = Monday
    const total = daysInMonth(viewYear, viewMonth);
    const prevM = viewMonth === 0 ? 11 : viewMonth - 1;
    const prevY = viewMonth === 0 ? viewYear - 1 : viewYear;
    const prevTotal = daysInMonth(prevY, prevM);
    const nextM = viewMonth === 11 ? 0 : viewMonth + 1;
    const nextY = viewMonth === 11 ? viewYear + 1 : viewYear;

    const cells: DayCell[] = [];
    for (let i = firstWeekday - 1; i >= 0; i--) {
      cells.push({ y: prevY, m: prevM, d: prevTotal - i, inMonth: false });
    }
    for (let d = 1; d <= total; d++) {
      cells.push({ y: viewYear, m: viewMonth, d, inMonth: true });
    }
    let nd = 1;
    while (cells.length < 42) {
      cells.push({ y: nextY, m: nextM, d: nd++, inMonth: false });
    }
    return cells;
  });
</script>

<svelte:window onclick={handleWindowClick} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="date-picker" bind:this={containerEl} onkeydown={handleKeydown}>
  <button type="button" class="trigger" class:active={open} onclick={toggle}>
    <Calendar size={14} />
    <span>{formatShort(value)}</span>
  </button>

  {#if open}
    <div class="dropdown">
      <div class="nav">
        <button
          type="button"
          class="nav-button"
          onclick={prevMonth}
          aria-label={t("component.previousMonth")}
        >
          <ChevronLeft size={16} />
        </button>
        <span class="nav-label">{MONTH_LABELS_FULL[viewMonth]} {viewYear}</span>
        <button
          type="button"
          class="nav-button"
          onclick={nextMonth}
          aria-label={t("component.nextMonth")}
        >
          <ChevronRight size={16} />
        </button>
      </div>

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
            class:selected={isSameDay(cell, selected)}
            onclick={() => pickDay(cell.y, cell.m, cell.d)}
          >
            {cell.d}
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .date-picker {
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
    padding: 0.45rem 0.5rem;
    font-size: 0.9rem;
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

  .day-cell.selected {
    background-color: var(--color-accent);
    color: var(--color-accent-contrast);
  }
</style>
