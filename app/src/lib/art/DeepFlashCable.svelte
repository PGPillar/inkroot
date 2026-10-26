<script lang="ts">
  import DitherDefs from './DitherDefs.svelte';

  type Part = 'button' | 'usbA' | 'usbC';
  let { width = 280, highlight, title }: { width?: number; highlight?: Part; title: string } = $props();

  // USB-A plug → cable → inline box with a round button → cable → USB-C plug. Centre line y=54.
  const uid = $props.id();
  const pid = `${uid}-dither`;
  const fill = (part: Part) => (highlight === part ? `url(#${pid})` : 'none');
  const EMPHASIS: Record<Part, string> = {
    usbA: 'M25 34v-6M13 35l-4-4M37 35l4-4',
    button: 'M146 26v-6M132 28l-4-4M160 28l4-4',
    usbC: 'M259 40v-6M249 41l-4-4M269 41l4-4',
  };
</script>

<svg role="img" aria-label={title} viewBox="0 0 280 96" {width} height={(width * 96) / 280}
  fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter">
  <DitherDefs id={pid} />
  <rect data-part="usbA" x="8" y="42" width="34" height="24" fill={fill('usbA')} />
  <path data-part="usbAHoles" d="M16 49h6v5h-6zM28 49h6v5h-6z" style="fill: var(--panel)" />
  <path data-part="usbABody" d="M42 38h32l4 4v24l-4 4H42zM54 47v14M60 47v14M66 47v14" />
  <path data-part="cable" d="M78 46l12 3v10l-12 3M90 54h28M174 54a40 40 0 0 0 32 0M206 49l10-3v16l-10-3" />
  <path data-part="box" d="M118 34h56v40h-56z" />
  <circle data-part="button" cx="146" cy="54" r="11" fill={fill('button')} />
  <path data-part="usbCBody" d="M216 42h28l4 4v16l-4 4h-28z" />
  <path data-part="usbC" d="M248 48h18l4 4v4l-4 4h-18z" fill={fill('usbC')} />
  {#if highlight}<path data-part="emphasis" d={EMPHASIS[highlight]} />{/if}
</svg>
