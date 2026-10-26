<script lang="ts">
  let {
    done = 0,
    total,
    segments = 24,
    meta,
  }: { done?: number; total?: number; segments?: number; meta?: string } = $props();

  const determinate = $derived(total !== undefined && total > 0);
  const ratio = $derived(determinate ? Math.min(1, Math.max(0, done / (total as number))) : 0);
  const filled = $derived(Math.floor(ratio * segments));
  const percent = $derived(Math.round(ratio * 100));
</script>

<div class="wrap">
  <div
    class="bar"
    class:marching={!determinate}
    role="progressbar"
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={determinate ? percent : undefined}
    style="--n:{segments}"
  >
    {#each Array.from({ length: segments }, (_, i) => i) as i (i)}
      <span
        class="seg"
        class:next={determinate && i === filled}
        data-filled={determinate && i < filled ? '' : undefined}
      ></span>
    {/each}
    {#if !determinate}<span class="hop"></span>{/if}
  </div>
  <div class="meta"><span>{meta ?? ''}</span><span></span></div>
</div>

<style>
  .bar {
    position: relative;
    display: flex;
    gap: 2px;
    height: 14px;
    padding: 2px;
    border: var(--border);
    overflow: hidden;
  }
  .seg {
    flex: 1;
    background: transparent;
  }
  .seg[data-filled] {
    background: var(--ink);
  }
  .seg.next {
    color: var(--ink);
    background: var(--dither-3);
  }
  .hop {
    position: absolute;
    top: 2px;
    bottom: 2px;
    left: 2px;
    width: calc((100% - 4px - (var(--n) - 1) * 2px) / var(--n));
    color: var(--ink);
    background: var(--dither-3);
    animation: hop 2.4s steps(var(--n), end) infinite;
  }
  @keyframes hop {
    to { transform: translateX(calc(var(--n) * (100% + 2px))); }
  }
  .meta {
    display: flex;
    justify-content: space-between;
    margin-top: var(--space-2);
    font: var(--fs-meta);
    color: var(--muted);
  }
  @media (prefers-reduced-motion: reduce) {
    .hop {
      animation: none;
    }
  }
</style>
