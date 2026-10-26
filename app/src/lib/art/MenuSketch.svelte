<script lang="ts">
  import DitherDefs from './DitherDefs.svelte';

  type Variant = 'onyxApps' | 'kebab' | 'toggle' | 'activityLauncher' | 'buildNumber' | 'devOptions';
  let {
    width = 176,
    highlight = 'target',
    variant = 'onyxApps',
    title,
  }: { width?: number; highlight?: 'target'; variant?: Variant; title: string } = $props();

  // A schematic settings screen: a top bar and five list rows. The row (or ⋮) to tap is dithered.
  const uid = $props.id();
  const pid = `${uid}-dither`;
  const dither = $derived(highlight === 'target' ? `url(#${pid})` : 'none');
  const ROWS = [28, 47, 66, 85, 104];
  const LEN = [52, 64, 44, 58, 48];
  const TARGET: Partial<Record<Variant, number>> = { onyxApps: 2, activityLauncher: 1, buildNumber: 3, devOptions: 2 };
  const target = $derived(TARGET[variant]);
  const icons = $derived(variant === 'onyxApps');
  const switches = $derived(variant === 'devOptions');
  const twoLine = $derived(variant === 'activityLauncher');
</script>

{#snippet toggle(x: number, y: number, on: boolean)}
  <rect {x} {y} width="20" height="10" style="fill: var(--panel)" />
  <rect x={on ? x + 10 : x} {y} width="10" height="10" fill={on ? 'currentColor' : 'none'} />
{/snippet}

<svg role="img" aria-label={title} viewBox="0 0 176 132" {width} height={(width * 132) / 176}
  fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter">
  <DitherDefs id={pid} />
  <rect data-part="frame" x="8" y="8" width="160" height="116" />
  {#if variant === 'activityLauncher'}
    <path data-part="search" d="M16 12h144v12H16zM21 17a3 3 0 1 0 6 0a3 3 0 1 0-6 0M27 20l2 2M36 18h40" />
  {:else}
    <path data-part="bar" d={`${variant === 'devOptions' ? 'M22 13l-5 5 5 5M30 18h40' : 'M18 18h44'}M8 28h160`} />
  {/if}
  {#each ROWS as y, i}
    {@const hit = target === i}
    <g data-part={hit ? 'target' : 'row'}>
      {#if hit}<rect x="9" y={y + 1} width="158" height="18" fill={dither} stroke="none" />{/if}
      {#if icons}<rect x="18" y={y + 6} width="8" height="8" style={hit ? 'fill: var(--panel)' : undefined} />{/if}
      {#if hit}<rect x={icons ? 33 : 15} y={y + 4} width={LEN[i] + 6} height="12" style="fill: var(--panel)" stroke="none" />{/if}
      <path d={twoLine ? `M18 ${y + 7}h${LEN[i]}M18 ${y + 13}h${LEN[i] - 20}` : `M${icons ? 36 : 18} ${y + 10}h${LEN[i]}`} />
      {#if switches}{@render toggle(138, y + 5, hit)}{/if}
      {#if variant === 'buildNumber' && hit}
        <rect x="104" y={y + 3} width="54" height="14" style="fill: var(--panel)" stroke="none" />
        <path d="M110 {y + 6}v8M117 {y + 6}v8M124 {y + 6}v8M131 {y + 6}v8M138 {y + 6}v8M145 {y + 6}v8M152 {y + 6}v8" />
      {/if}
      {#if i < ROWS.length - 1}<path d={`M8 ${y + 19}h160`} />{/if}
    </g>
  {/each}
  {#if variant === 'kebab'}
    <rect data-part="target" x="138" y="9" width="29" height="18" fill={dither} stroke="none" />
    <rect data-part="kebab" x="147" y="9" width="11" height="18" style="fill: var(--panel)" stroke="none" />
    <path data-part="kebabDots" d="M151 11h3v3h-3zM151 16.5h3v3h-3zM151 22h3v3h-3z" fill="currentColor" stroke="none" />
  {:else if variant === 'toggle'}
    <g data-part="dropdown">
      <rect x="84" y="20" width="80" height="58" style="fill: var(--panel)" />
      <path d="M92 32h40M84 39h80M84 58h80M92 68h34" />
      <rect x="85" y="40" width="78" height="17" fill={dither} stroke="none" />
      <rect x="89" y="44" width="40" height="10" style="fill: var(--panel)" stroke="none" />
      <path d="M92 49h34" />
      {@render toggle(138, 44, true)}
    </g>
  {/if}
</svg>
