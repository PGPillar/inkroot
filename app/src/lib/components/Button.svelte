<script lang="ts">
  import type { Snippet } from 'svelte';
  import DitherIcon from './DitherIcon.svelte';

  let {
    variant = 'primary',
    disabled = false,
    onclick,
    type = 'button',
    children,
  }: {
    variant?: 'primary' | 'secondary' | 'danger';
    disabled?: boolean;
    onclick?: () => void;
    type?: 'button' | 'submit';
    children: Snippet;
  } = $props();

  function handle(e: MouseEvent) {
    if (disabled) {
      e.preventDefault();
      return;
    }
    onclick?.();
  }
</script>

<button class="btn {variant}" class:disabled {type} {disabled} aria-disabled={disabled} onclick={handle}>
  {#if variant === 'danger'}<DitherIcon name="warning" size={16} cell={2} />{/if}
  {@render children()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: 12px 18px;
    font: 600 15px var(--font-sans);
    border: var(--border);
    border-radius: 0;
    cursor: pointer;
    background: transparent;
    color: var(--ink);
  }
  .primary {
    background: var(--ink);
    color: var(--panel);
  }
  .danger {
    border: var(--border-heavy);
    font-weight: 600;
  }
  .btn.disabled {
    border-style: dashed;
    background: transparent;
    color: var(--muted);
    cursor: not-allowed;
  }
  .btn:focus-visible {
    outline: var(--border);
    outline-offset: 3px;
  }
</style>
