<script lang="ts">
  /** The app's checkbox: a real (visually hidden) input for accessibility,
   * with the visible box drawn here so it can carry the accent fill and the
   * tri-state tick the native widget can't.
   *
   * Extracted from the transactions list so the CSV import preview gets the
   * same control rather than a second one that behaves almost-but-not-quite
   * the same — both lists are "tick the rows you mean".
   *
   * Visibility is the caller's business: set `--checkbox-opacity: 0` on an
   * ancestor to hide it until hover (what the transactions list does, so a
   * selection column doesn't clutter every row for a feature most visits
   * never use). A checked, indeterminate or focused box always shows itself
   * regardless, or it could be ticked and invisible. CSS custom properties
   * are used rather than a class prop because they cross the component
   * boundary that Svelte's scoped styles deliberately don't.
   */
  import { Check, Minus } from "@lucide/svelte";

  let {
    checked,
    indeterminate = false,
    disabled = false,
    /** `sm` for dense inline settings lists; `md` (the default) for the
     * selection column of a table, where it's a primary click target. */
    size = "md",
    ariaLabel,
    onpress,
  }: {
    checked: boolean;
    indeterminate?: boolean;
    disabled?: boolean;
    size?: "sm" | "md";
    ariaLabel: string;
    onpress: (event: MouseEvent) => void;
  } = $props();

  /** Keeps the native `indeterminate` visual state in sync — there is no
   * HTML attribute for it, only the DOM property. Applied to the real input
   * so screen readers still get it. */
  function setIndeterminate(node: HTMLInputElement, value: boolean) {
    node.indeterminate = value;
    return {
      update(value: boolean) {
        node.indeterminate = value;
      },
    };
  }
</script>

<label class="checkbox" class:checked class:indeterminate class:disabled>
  <input
    type="checkbox"
    {checked}
    {disabled}
    use:setIndeterminate={indeterminate}
    aria-label={ariaLabel}
    onmousedown={(event) => {
      if (disabled) return;
      // Fires on mousedown, not click, for two reasons: a browser starts
      // extending its own text selection right here, before any click
      // handler would even run, so preventing default is what stops a
      // shift-click (or a drag) from also sweeping up the row text as a
      // selection — and a following drag needs the state already applied by
      // the time the cursor reaches the next row, not a beat later.
      event.preventDefault();
      onpress(event);
    }}
    onclick={(event) => {
      // A checkbox's own checked-state flip is tied to the `click` event
      // specifically (browsers pre-toggle it before dispatch, then revert if
      // the click is prevented) — preventing mousedown's default above
      // doesn't touch that. Without this, the native toggle fires right
      // alongside our own, fighting `checked` for which one wins. All the
      // actual logic already ran on mousedown; this is just here to keep the
      // native behavior out of the way.
      event.preventDefault();
    }}
  />
  <span class="box" class:sm={size === "sm"}>
    {#if indeterminate}
      <Minus size={size === "sm" ? 11 : 13} strokeWidth={3} />
    {:else if checked}
      <Check size={size === "sm" ? 11 : 13} strokeWidth={3} />
    {/if}
  </span>
</label>

<style>
  .checkbox {
    display: inline-flex;
    position: relative;
    cursor: pointer;
    opacity: var(--checkbox-opacity, 1);
    transition: opacity 0.1s;
  }

  /* Whatever the caller asked for, a box that is ticked or focused must be
     visible — otherwise a row can be selected with nothing on screen saying
     so. */
  .checkbox.checked,
  .checkbox.indeterminate,
  .checkbox:focus-within {
    opacity: 1;
  }

  .checkbox.disabled {
    cursor: default;
    opacity: 0.35;
  }

  .checkbox input {
    /* Without this, some engines keep hit-testing against the native
       checkbox widget's own default-sized hotspot instead of the CSS box
       `inset: 0` stretches it to — the visible part is all drawn by `.box`
       below anyway, so the input has no native look left to preserve. */
    appearance: none;
    -webkit-appearance: none;
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
    cursor: inherit;
  }

  .box {
    width: 1.35rem;
    height: 1.35rem;
    border-radius: 5px;
    border: 1.5px solid var(--color-shade-4);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent-contrast);
    pointer-events: none;
  }

  .box.sm {
    width: 1.05rem;
    height: 1.05rem;
    border-radius: 4px;
  }

  .checkbox.checked .box,
  .checkbox.indeterminate .box {
    background-color: var(--color-accent);
    border-color: var(--color-accent);
  }
</style>
