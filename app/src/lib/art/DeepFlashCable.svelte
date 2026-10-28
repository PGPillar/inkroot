<script lang="ts">
  import DitherDefs from './DitherDefs.svelte';

  type Cue = 'hold' | 'none';
  let {
    width = 480,
    cue = 'hold',
    labels = true,
    tabletLabel = false,
    title,
  }: {
    width?: number;
    /** hold: the website's down arrow and HOLD over the button. */
    cue?: Cue;
    labels?: boolean;
    /** Label the right-hand end TABLET (a scene draws the tablet there) instead of USB-C → TABLET. */
    tabletLabel?: boolean;
    title: string;
  } = $props();

  // The website's cable drawing (site/rescue/index.html), shape for shape: USB-A shell with its
  // tongue, a 50% dithered grip, straight cable, a 25% dithered button box with a solid button,
  // straight cable, a 50% dithered grip, a short USB-C shell. Centre line y=88.
  const uid = $props.id();
  const d50 = `${uid}-d50`;
  const d25 = `${uid}-d25`;
</script>

<svg role="img" aria-label={title} viewBox="0 0 480 176" {width} height={(width * 176) / 480}
  fill="none" stroke="currentColor" stroke-width="2"
  shape-rendering="crispEdges">
  <DitherDefs id={d50} />
  <defs>
    <pattern id={d25} width="4" height="4" patternUnits="userSpaceOnUse">
      <rect width="2" height="2" fill="currentColor" stroke="none" />
    </pattern>
  </defs>
  <rect data-part="usbA" x="9" y="69" width="50" height="38" />
  <rect data-part="usbATongue" x="17" y="81" width="34" height="8" fill="currentColor" />
  <rect data-part="usbAGrip" x="59" y="61" width="52" height="54" fill="url(#{d50})" />
  <path data-part="cable" d="M111 88H189M291 88H371" />
  <rect data-part="box" x="189" y="61" width="102" height="54" fill="url(#{d25})" />
  <rect data-part="button" x="219" y="73" width="42" height="30" fill="currentColor" />
  <rect data-part="usbCGrip" x="371" y="65" width="46" height="46" fill="url(#{d50})" />
  <rect data-part="usbC" x="417" y="76" width="44" height="24" />
  <rect data-part="usbCTongue" x="425" y="85" width="28" height="6" fill="currentColor" />
  {#if cue === 'hold'}
    <path data-part="holdArrow" d="M240 13V53M231 44l9 9 9-9" />
  {/if}
  {#if labels || cue === 'hold'}
    <g data-part="labels" fill="currentColor" stroke="none" font-size="13" letter-spacing="1.5"
      style="font-family: var(--font-mono, monospace)" shape-rendering="auto">
      {#if cue === 'hold'}<text x="256" y="30">HOLD</text>{/if}
      {#if labels}
        <text x="9" y="148">USB-A</text><text x="9" y="166">→ COMPUTER</text>
        <text x="240" y="148" text-anchor="middle">BUTTON</text>
        {#if !tabletLabel}
          <text x="471" y="148" text-anchor="end">USB-C</text><text x="471" y="166" text-anchor="end">→ TABLET</text>
        {/if}
      {/if}
    </g>
  {/if}
</svg>
