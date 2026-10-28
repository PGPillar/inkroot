<script lang="ts">
  import DitherDefs from './DitherDefs.svelte';

  let { width = 480, title }: { width?: number; title: string } = $props();

  // Left: the cable's USB-A plug (the website cable's shell, tongue and grip) pushed into a USB-A
  // port on the computer itself. Right: a USB-C→A adapter, struck out. Same units,
  // centre line and mono labels as DeepFlashCable; the frame starts at y=41, as nothing sits above the cable.
  const uid = $props.id();
  const d50 = `${uid}-d50`;
  const d25 = `${uid}-d25`;
</script>

<svg role="img" aria-label={title} viewBox="0 41 480 135" {width} height={(width * 135) / 480}
  fill="none" stroke="currentColor" stroke-width="2" shape-rendering="crispEdges">
  <DitherDefs id={d50} />
  <defs>
    <pattern id={d25} width="4" height="4" patternUnits="userSpaceOnUse">
      <rect width="2" height="2" fill="currentColor" stroke="none" />
    </pattern>
  </defs>
  <rect data-part="computer" x="9" y="49" width="130" height="78" fill="url(#{d25})" />
  <rect data-part="port" x="99" y="65" width="40" height="46" style="fill: var(--panel)" />
  <rect data-part="usbA" x="109" y="69" width="50" height="38" style="fill: var(--panel)" />
  <rect data-part="usbATongue" x="117" y="81" width="34" height="8" fill="currentColor" />
  <rect data-part="usbAGrip" x="159" y="61" width="52" height="54" fill="url(#{d50})" />
  <path data-part="cable" d="M211 88H247" />
  <rect data-part="adapterUsbC" x="325" y="76" width="44" height="24" />
  <rect data-part="adapterUsbCTongue" x="333" y="85" width="28" height="6" fill="currentColor" />
  <rect data-part="adapter" x="369" y="61" width="52" height="54" fill="url(#{d50})" />
  <rect data-part="adapterUsbA" x="421" y="69" width="50" height="38" />
  <rect data-part="adapterUsbATongue" x="429" y="81" width="34" height="8" fill="currentColor" />
  <path data-part="cross" d="M323 51l146 74M323 125l146-74" stroke-width="4" shape-rendering="auto" />
  <g data-part="labels" fill="currentColor" stroke="none" font-size="13" letter-spacing="1.5"
    style="font-family: var(--font-mono, monospace)" shape-rendering="auto">
    <text x="9" y="148">USB-A PORT</text><text x="9" y="166">ON THE COMPUTER</text>
    <text x="471" y="148" text-anchor="end">NO ADAPTER</text><text x="471" y="166" text-anchor="end">NO HUB</text>
  </g>
</svg>
