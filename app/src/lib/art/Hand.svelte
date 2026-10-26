<script lang="ts">
  import DitherDefs from './DitherDefs.svelte';

  type Part = 'finger' | 'cuff';
  type Pose = 'press' | 'hold';
  let {
    width = 96,
    highlight,
    pose = 'press',
    title,
  }: { width?: number; highlight?: Part; pose?: Pose; title: string } = $props();

  // A hand pointing down; the fingertip sits at the bottom centre (48, 134) so it can be
  // stacked straight above a button or a PressIndicator.
  const uid = $props.id();
  const pid = `${uid}-dither`;
  // Opaque panel-coloured contour behind the outline, so the hand hides any art it is laid over.
  const PALM = 'M22 4H74V22L78 36V68A5 5 0 0 1 68 68V70A5 5 0 0 1 58 70';
  const THUMB = 'V80H34L20 62V48L26 40V22H22Z';
  const SILHOUETTE: Record<Pose, string> = {
    press: `${PALM}V124A10 10 0 0 1 38 124${THUMB}`,
    hold: `${PALM}V126L53 133H43L38 126${THUMB}`,
  };
  const fill = (part: Part) => (highlight === part ? `url(#${pid})` : 'none');
</script>

<svg role="img" aria-label={title} viewBox="0 0 96 140" {width} height={(width * 140) / 96}
  fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter">
  <DitherDefs id={pid} />
  <path data-part="silhouette" d={SILHOUETTE[pose]} style="fill: var(--panel)" stroke="none" />
  <path data-part="cuff" d="M22 4h52v18H22zM22 16h52" fill={fill('cuff')} />
  <path data-part="hand" d="M26 22v18l-6 8v14l14 18h4M70 22l8 14v32M78 60v8a5 5 0 0 1-10 0v-4M68 66v4a5 5 0 0 1-10 0v-2" />
  {#if pose === 'press'}
    <path data-part="finger" d="M38 80v44a10 10 0 0 0 20 0V70" fill={fill('finger')} />
    <path data-part="nail" d="M42 116h12v6a6 6 0 0 1-12 0z" />
    <path data-part="motion" d="M16 96l6 6 6-6M16 108l6 6 6-6M68 96l6 6 6-6M68 108l6 6 6-6" />
  {:else}
    <path data-part="finger" d="M38 80v46l5 7h10l5-7V70" fill={fill('finger')} />
    <path data-part="nail" d="M42 118h12v8H42z" />
    <path data-part="surface" d="M16 135h64M26 129l-6-4M70 129l6-4" />
  {/if}
  <path data-part="crease" d="M42 96h12" />
</svg>
