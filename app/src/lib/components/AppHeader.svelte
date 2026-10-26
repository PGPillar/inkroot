<script lang="ts">
  import { ABOUT_COPY, COMMON } from '../copy';
  import type { DeviceStatus } from '../ipc';
  import DeviceChip from './DeviceChip.svelte';

  let {
    device,
    demo,
    navLocked,
    onHome,
    onNav,
  }: {
    device: DeviceStatus;
    demo: boolean;
    navLocked: boolean;
    onHome: () => void;
    onNav: (s: 'backups' | 'about') => void;
  } = $props();
</script>

<header>
  <button type="button" class="logo" onclick={onHome}>
    <span class="mk" aria-hidden="true"></span>{COMMON.wordmark}
  </button>
  {#if !navLocked}
    <nav>
      <button type="button" class="link" onclick={() => onNav('backups')}>{COMMON.backups}</button>
      <button type="button" class="link" onclick={() => onNav('about')}>{COMMON.about}</button>
    </nav>
  {/if}
  <div class="right">
    {#if demo}<span class="demo">{ABOUT_COPY.demoChip}</span>{/if}
    <DeviceChip status={device} />
  </div>
</header>

<style>
  header {
    display: flex;
    align-items: center;
    gap: var(--space-5);
    padding: var(--space-3) var(--gutter);
    border-bottom: var(--border);
  }
  button {
    background: transparent;
    border: 0;
    color: var(--ink);
    cursor: pointer;
    padding: var(--space-1) 0;
  }
  .logo {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font: var(--fs-wordmark);
    letter-spacing: var(--track-wordmark);
  }
  .mk {
    width: 16px;
    height: 16px;
    box-sizing: border-box;
    border: var(--border);
    background: var(--dither-4);
  }
  nav {
    display: flex;
    gap: var(--space-4);
  }
  .link {
    font: 500 14px var(--font-sans);
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  button:focus-visible {
    outline: var(--border);
    outline-offset: 3px;
  }
  .right {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .demo {
    font: var(--fs-chip);
    padding: var(--pad-chip);
    border: var(--border);
    white-space: nowrap;
  }
</style>
