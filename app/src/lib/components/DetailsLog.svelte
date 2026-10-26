<script lang="ts">
  import { tick } from 'svelte';
  import { COMMON } from '../copy';
  import type { LogLine } from '../ipc';

  let {
    lines,
    tech,
    onCopy,
  }: { lines: LogLine[]; tech?: string; onCopy: () => Promise<boolean> } = $props();

  let open = $state(false);
  let status = $state<'idle' | 'copied' | 'failed'>('idle');
  let pre: HTMLPreElement | undefined = $state();
  let timer: ReturnType<typeof setTimeout> | undefined;

  const text = $derived(
    [
      ...(tech ? [tech] : []),
      ...lines.slice(-500).map((l) => `${l.ts.slice(11, 19)} ${l.stream === 'stderr' ? '!' : ' '} ${l.line}`),
    ].join('\n'),
  );

  $effect(() => {
    void text;
    if (!open) return;
    tick().then(() => {
      if (pre) pre.scrollTop = pre.scrollHeight;
    });
  });

  async function copy() {
    const ok = await onCopy();
    status = ok ? 'copied' : 'failed';
    clearTimeout(timer);
    timer = setTimeout(() => (status = 'idle'), 2000);
  }

  $effect(() => () => clearTimeout(timer));
</script>

<section class="details">
  <div class="row">
    <button type="button" class="toggle" aria-expanded={open} onclick={() => (open = !open)}>
      {COMMON.details} {open ? '▾' : '▸'}
    </button>
    {#if open}
      <button type="button" class="copy" onclick={copy}>{COMMON.copyLog}</button>
      {#if status !== 'idle'}
        <span class="status" role="status">{status === 'copied' ? COMMON.copied : COMMON.copyFailed}</span>
      {/if}
    {/if}
  </div>
  {#if open}
    <pre bind:this={pre}>{text}</pre>
  {/if}
</section>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  button {
    font: var(--fs-meta);
    color: var(--ink);
    background: transparent;
    border: 0;
    padding: var(--space-1) 0;
    cursor: pointer;
  }
  .copy {
    border: var(--border);
    padding: var(--space-1) var(--space-2);
  }
  .status {
    font: var(--fs-meta);
    color: var(--muted);
  }
  pre {
    margin: var(--space-2) 0 0;
    padding: var(--space-2) var(--space-3);
    border: var(--border);
    max-height: 180px;
    overflow: auto;
    font: var(--fs-meta);
    white-space: pre-wrap;
    word-break: break-all;
    user-select: text;
  }
</style>
