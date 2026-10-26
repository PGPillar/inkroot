<script lang="ts">
  import { tick } from 'svelte';
  import { CLOSE_GUARD_COPY } from '../copy';
  import Button from './Button.svelte';
  import DitherIcon from './DitherIcon.svelte';

  let { onKeep, onClose }: { onKeep: () => void; onClose: () => void } = $props();

  const uid = $props.id();
  let dialog: HTMLElement | undefined = $state();

  const buttons = () => [...(dialog?.querySelectorAll<HTMLButtonElement>('button') ?? [])];

  // Whatever had focus before the dialog gets it back when the dialog goes (after the page
  // behind it stops being inert).
  const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;

  // Keep Inkroot open is focused first, and is first in tab order; the danger button never is.
  $effect(() => {
    buttons()[0]?.focus();
    return () => void tick().then(() => previous?.isConnected && previous.focus());
  });

  /** Tab and Shift+Tab cycle between the two buttons; Escape keeps Inkroot open. */
  function keydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onKeep();
      return;
    }
    if (e.key !== 'Tab') return;
    const all = buttons();
    const i = all.indexOf(document.activeElement as HTMLButtonElement);
    const next = (i + (e.shiftKey ? -1 : 1) + all.length) % all.length;
    e.preventDefault();
    all[next]?.focus();
  }
</script>

<div class="scrim">
  <div
    bind:this={dialog}
    class="dialog"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="{uid}-h"
    aria-describedby="{uid}-p"
    tabindex="-1"
    onkeydown={keydown}
  >
    <DitherIcon name="warning" size={40} cell={2} />
    <h2 id="{uid}-h">{CLOSE_GUARD_COPY.heading}</h2>
    <p id="{uid}-p">{CLOSE_GUARD_COPY.body}</p>
    <div class="buttons">
      <Button onclick={onKeep}>{CLOSE_GUARD_COPY.keep}</Button>
      <Button variant="danger" onclick={onClose}>{CLOSE_GUARD_COPY.closeAnyway}</Button>
    </div>
  </div>
</div>

<style>
  /* A 1-bit 50 % screen in the panel colour over the app: the e-ink way to dim a page. */
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    padding: var(--gutter);
    background: repeating-conic-gradient(var(--panel) 0 25%, transparent 0 50%) 0 0 / 2px 2px;
  }
  .dialog {
    width: min(520px, 100%);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-5);
    background: var(--panel);
    border: var(--border-heavy);
    outline: none;
  }
  h2 {
    margin: var(--space-1) 0 0;
    font: 600 22px/1.2 var(--font-sans);
    letter-spacing: var(--track-heading);
  }
  p {
    margin: 0;
    font: var(--fs-body);
  }
  /* DOM order keeps Keep first for focus; it sits on the right, where the primary always is. */
  .buttons {
    display: flex;
    flex-direction: row-reverse;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-3);
  }
</style>
