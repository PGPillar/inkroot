<script lang="ts" module>
  /** One illustration per cable-guide panel (c1-c6). */
  export type CableSceneId = 'cable' | 'port' | 'press' | 'plug' | 'power' | 'hold';
</script>

<script lang="ts">
  import DeepFlashCable from './DeepFlashCable.svelte';
  import UsbAPort from './UsbAPort.svelte';

  let { scene, seconds, title }: { scene: CableSceneId; seconds?: number; title: string } = $props();

  // Every scene is the website's cable (480×176, centre line y=88) drawn 1:1, plus a few
  // Panel-style additions in the same units and the website's own HOLD arrow: a heavier arrow
  // while the button is held, the tablet on the USB-C end, a second arrow on its power key.
  // Drawn at 1:1 so the 2 px strokes stay crisp at 1×.
  const H = 176;
  const W: Record<CableSceneId, number> = { cable: 480, port: 480, press: 480, plug: 660, power: 660, hold: 660 };
  const width = $derived(W[scene]);
  const tablet = $derived(scene === 'plug' || scene === 'power' || scene === 'hold');
  /** The panel's own action gets the heavy (4 px) arrow; the button still held behind it keeps 2 px. */
  const heavyHold = $derived(scene === 'press' || scene === 'hold');
  const LABEL = 'font-family: var(--font-mono, monospace)';
</script>

<svg class="scene" role="img" aria-label={title} data-scene={scene} viewBox="0 0 {width} {H}" {width} height={H}
  fill="none" stroke="currentColor" stroke-width="2" shape-rendering="crispEdges">
  {#if scene === 'port'}
    <g transform="translate(0 41)"><UsbAPort {title} /></g>
  {:else}
    <DeepFlashCable cue={scene === 'cable' ? 'hold' : 'none'} tabletLabel={tablet} {title} />
  {/if}

  {#if tablet}
    <!-- The tablet on its side: the USB-C shell goes half into the port on its left edge;
         the power key sits on its right edge, level with the cable. -->
    <g data-part="tablet">
      <rect x="441" y="21" width="150" height="110" style="fill: var(--panel)" />
      <rect x="455" y="33" width="122" height="86" />
      <rect x="591" y="72" width="6" height="32" fill="currentColor" />
    </g>
  {/if}
  {#if scene === 'plug'}
    <path data-part="plugArrow" d="M371 39H425M416 30l9 9-9 9" stroke-width="4" />
  {/if}
  {#if scene !== 'cable' && scene !== 'port'}
    <path data-part="holdArrow" d="M240 13V53M231 44l9 9 9-9" stroke-width={heavyHold ? 4 : 2} />
  {/if}
  {#if scene === 'power'}
    <path data-part="powerArrow" d="M651 88H607M616 79l-9 9 9 9" stroke-width="4" />
  {/if}

  {#if scene !== 'cable' && scene !== 'port'}
    <g data-part="labels" fill="currentColor" stroke="none" font-size="13" letter-spacing="1.5" style={LABEL}
      shape-rendering="auto">
      <text x="256" y="30" font-weight={heavyHold ? 600 : 400}>HOLD</text>
      {#if scene === 'plug'}<text x="371" y="22" font-weight="600">PLUG IN</text>{/if}
      {#if scene === 'power'}<text x="651" y="66" text-anchor="end" font-weight="600">HOLD</text>{/if}
      {#if tablet}<text x="516" y="148" text-anchor="middle">TABLET</text>{/if}
    </g>
  {/if}
  {#if seconds !== undefined}
    <text data-part="seconds" x={scene === 'power' ? 651 : 256} y={scene === 'power' ? 124 : 54}
      text-anchor={scene === 'power' ? 'end' : 'start'} fill="currentColor" stroke="none" shape-rendering="auto"
      style="font: 600 18px var(--font-mono, monospace)">{seconds} s</text>
  {/if}
</svg>

<style>
  .scene {
    display: block;
    max-width: 100%;
    height: auto;
    overflow: visible;
  }
</style>
