<script lang="ts">
  import DitherDefs from './DitherDefs.svelte';

  type Part = 'port' | 'adapter';
  type Variant = 'port' | 'adapterCrossed';
  let {
    width,
    highlight,
    variant = 'port',
    title,
  }: { width?: number; highlight?: Part; variant?: Variant; title: string } = $props();

  // The side of a closed laptop with its ports. adapterCrossed plugs a USB-C→A dongle into its
  // USB-C port and strikes the dongle out.
  const uid = $props.id();
  const pid = `${uid}-dither`;
  const vbH = $derived(variant === 'adapterCrossed' ? 140 : 112);
  const w = $derived(width ?? 200);
  const fill = (part: Part) => (highlight === part ? `url(#${pid})` : 'none');
</script>

<svg role="img" aria-label={title} viewBox="0 0 200 {vbH}" width={w} height={(w * vbH) / 200}
  fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter">
  <DitherDefs id={pid} />
  <path data-part="lid" d="M12 24h176l4 4v2l-4 4H12l-4-4v-2z" />
  <path data-part="base" d="M12 38h176l4 4v40l-4 4H12l-4-4V42zM16 86v4h10v-4M174 86v4h10v-4" />
  <rect data-part="port" x="28" y="52" width="44" height="20" fill={fill('port')} />
  <rect data-part="tongue" x="34" y="56" width="32" height="6" fill="currentColor" />
  <path data-part="usbC" d="M92 58h18l3 3v2l-3 3H92l-3-3v-2z" />
  <circle data-part="jack" cx="132" cy="62" r="5" />
  <path data-part="vents" d="M150 56v12M158 56v12M166 56v12M174 56v12" />
  {#if highlight === 'port'}<path data-part="emphasis" d="M50 96v6M38 95l-4 4M62 95l4 4" />{/if}
  {#if variant === 'adapterCrossed'}
    <!-- A USB-C→A dongle plugged into the laptop's own USB-C port, hanging below it, struck out. -->
    <g data-part="adapter">
      <path d="M86 54h30v16H86z" style="fill: var(--panel)" />
      <path d="M95 58v8M101 58v8M107 58v8M101 70v42h23" />
      <path d="M124 98h44l4 4v20l-4 4h-44z" fill={fill('adapter')} />
      <rect x="132" y="104" width="32" height="16" style="fill: var(--panel)" />
      <rect x="137" y="108" width="22" height="4" fill="currentColor" stroke="none" />
    </g>
    <path data-part="cross" d="M94 90l24 24M94 114l24-24" stroke-width="4" />
  {/if}
</svg>
