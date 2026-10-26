<script lang="ts">
  import DitherIcon from './DitherIcon.svelte';
  import type { IconName } from './icons';

  let {
    title,
    desc,
    icon,
    primary = false,
    disabled = false,
    size = 'normal',
    onclick,
  }: {
    title: string;
    desc: string;
    icon: IconName;
    primary?: boolean;
    disabled?: boolean;
    /** hero: the big Home / Already-rooted cards, contents scaled up with the card (mockup .B .ch). */
    size?: 'normal' | 'hero';
    onclick: () => void;
  } = $props();
</script>

<button class="card" class:primary class:hero={size === 'hero'} {disabled} type="button" {onclick}>
  <DitherIcon name={icon} size={size === 'hero' ? 80 : 40} />
  <span class="t">{title}</span>
  <span class="d">{desc}</span>
</button>

<style>
  .card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    min-height: 150px;
    padding: 14px 12px 12px;
    border: var(--border);
    border-radius: 0;
    background: var(--panel);
    color: var(--ink);
    text-align: left;
    cursor: pointer;
  }
  .card.primary {
    background: var(--ink);
    color: var(--panel);
  }
  .card:disabled {
    border-style: dashed;
    color: var(--muted);
    cursor: not-allowed;
  }
  .card:focus-visible {
    outline: var(--border);
    outline-offset: 3px;
  }
  .t {
    font: 600 16px var(--font-sans);
    margin-top: auto;
  }
  .d {
    font: 400 12px var(--font-mono);
    opacity: 0.7;
  }
  .hero {
    gap: 10px;
    min-height: 0;
    padding: 22px 20px 20px;
  }
  .hero .t {
    font: 600 24px/1.15 var(--font-sans);
  }
  .hero .d {
    font: 400 14px var(--font-mono);
  }
</style>
