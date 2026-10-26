<script lang="ts">
  let { state, text }: { state: 'waiting' | 'found' | 'attention'; text: string } = $props();
  // Copy that already ends in an ellipsis ("waiting for tablet…") doesn't get a second one.
  const dots = $derived(state === 'waiting' && !text.endsWith('…'));
  // Nor does copy that already carries its own tick ("Found it ✓").
  const tick = $derived(state === 'found' && !text.includes('✓'));
</script>

<p class="live" aria-live="polite">
  {#if tick}<span aria-hidden="true">✓</span>{:else if state === 'attention'}<span aria-hidden="true">●</span>{/if}
  <span>{text}</span>
  {#if dots}<span class="dots" aria-hidden="true"><i>.</i><i>.</i><i>.</i></span>{/if}
</p>

<style>
  .live {
    margin: 0;
    display: flex;
    gap: var(--space-2);
    font: var(--fs-meta);
  }
  .dots i {
    font-style: normal;
    animation: dot 1.2s steps(1, end) infinite;
  }
  .dots i:nth-child(2) { animation-delay: 0.4s; }
  .dots i:nth-child(3) { animation-delay: 0.8s; }
  @keyframes dot {
    0% { opacity: 0; }
    33% { opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .dots i { animation: none; }
  }
</style>
