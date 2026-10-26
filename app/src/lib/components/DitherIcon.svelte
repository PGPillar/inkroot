<script lang="ts">
  import { ICON_PATHS, type IconName } from './icons';

  let { name, size = 34, cell = 3 }: { name: IconName; size?: number; cell?: 2 | 3 | 4 } = $props();

  const mask = $derived(
    `url("data:image/svg+xml,${encodeURIComponent(
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="${ICON_PATHS[name]}"/></svg>`,
    )}")`,
  );
</script>

<span
  class="ico"
  aria-hidden="true"
  data-icon={name}
  style="--m:{mask};--size:{size}px;--cell:{cell}px"
></span>

<style>
  .ico {
    display: inline-block;
    flex: none;
    width: var(--size);
    height: var(--size);
    background: repeating-conic-gradient(currentColor 0 25%, transparent 0 50%) 0 0 / var(--cell) var(--cell);
    opacity: 0.85;
    -webkit-mask: var(--m) center / contain no-repeat;
    mask: var(--m) center / contain no-repeat;
  }
</style>
