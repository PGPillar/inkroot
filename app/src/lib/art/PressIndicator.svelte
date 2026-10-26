<script lang="ts">
  import DitherDefs from './DitherDefs.svelte';

  type Part = 'point';
  let {
    width = 96,
    highlight = 'point',
    seconds,
    title,
  }: { width?: number; highlight?: Part; seconds?: number; title: string } = $props();

  // A press point with ripples on both sides, stepping from solid to sparse dashes.
  const uid = $props.id();
  const pid = `${uid}-dither`;
  const C = 48;
  const SPAN = (35 * Math.PI) / 180;
  const RINGS = [
    { r: 16, dash: undefined },
    { r: 26, dash: '6 4' },
    { r: 36, dash: '2 5' },
  ];
  const arc = (r: number) => {
    const dx = +(r * Math.cos(SPAN)).toFixed(1);
    const dy = +(r * Math.sin(SPAN)).toFixed(1);
    const right = `M${C + dx} ${C - dy}A${r} ${r} 0 0 1 ${C + dx} ${C + dy}`;
    const left = `M${C - dx} ${C + dy}A${r} ${r} 0 0 1 ${C - dx} ${C - dy}`;
    return right + left;
  };
  const vbH = $derived(seconds === undefined ? 96 : 124);
</script>

<svg role="img" aria-label={title} viewBox="0 0 96 {vbH}" {width} height={(width * vbH) / 96}
  fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter">
  <DitherDefs id={pid} />
  <circle data-part="point" cx={C} cy={C} r="10" fill={highlight === 'point' ? `url(#${pid})` : 'none'} />
  {#each RINGS as ring}
    <path data-part="ripple" d={arc(ring.r)} stroke-dasharray={ring.dash} stroke-linecap="butt" />
  {/each}
  {#if seconds !== undefined}
    <text data-part="seconds" x={C} y="114" text-anchor="middle" fill="currentColor" stroke="none"
      style="font: 600 18px var(--font-mono, monospace)">{seconds} s</text>
  {/if}
</svg>
