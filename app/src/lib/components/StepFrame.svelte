<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    label,
    heading,
    body,
    art,
    children,
    actions,
    tone = 'normal',
  }: {
    label: string;
    heading: string;
    body?: string;
    art?: Snippet;
    children?: Snippet;
    actions?: Snippet;
    tone?: 'normal' | 'danger';
  } = $props();
</script>

<section class="frame" class:danger={tone === 'danger'}>
  <div class="label">{label}</div>
  <h1>{heading}</h1>
  {#if body}<p>{body}</p>{/if}
  {#if art}<div class="art">{@render art()}</div>{:else}<div class="spacer"></div>{/if}
  {@render children?.()}
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</section>

<style>
  .frame {
    box-sizing: border-box;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-5) var(--gutter);
  }
  .frame.danger {
    border: var(--border-heavy);
    margin: var(--space-3);
    padding: var(--space-4) var(--space-5);
  }
  .label {
    font: var(--fs-label);
    letter-spacing: var(--track-label);
    text-transform: uppercase;
    opacity: var(--opacity-label);
  }
  h1 {
    margin: var(--space-2) 0 0;
    font: var(--fs-heading);
    letter-spacing: var(--track-heading);
  }
  p {
    margin: 0;
    max-width: 52ch;
    font: var(--fs-body);
  }
  /* The art slot grows into spare room but never shrinks below its drawing: when space runs out
     (e.g. the Details log is open) the page scrolls instead of the drawing sliding under the text. */
  .art,
  .spacer {
    flex: 1 0 auto;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .actions {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    justify-content: flex-end;
    margin-top: var(--space-3);
  }
  .actions :global(> :first-child:not(:last-child)) {
    margin-right: auto;
  }
</style>
