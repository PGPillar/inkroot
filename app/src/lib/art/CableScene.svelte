<script lang="ts" module>
  /** One illustration per cable-guide panel (c1-c6). */
  export type CableSceneId = 'cable' | 'port' | 'press' | 'plug' | 'power' | 'hold';
</script>

<script lang="ts">
  import { CABLE_BUTTON, CABLE_USB_C_TIP, HAND_TIP, PRESS_POINT, TABLET_PORT, TABLET_POWER_EDGE, VIEWBOX } from './anchors';
  import DeepFlashCable from './DeepFlashCable.svelte';
  import Hand from './Hand.svelte';
  import PressIndicator from './PressIndicator.svelte';
  import Tablet from './Tablet.svelte';
  import UsbAPort from './UsbAPort.svelte';

  let {
    scene,
    seconds,
    height,
    title,
  }: { scene: CableSceneId; seconds?: number; height: number; title: string } = $props();

  // Every scene is laid out in its own user space from the anchors, then scaled as a whole,
  // so the fingertip always lands on the button whatever size the scene is drawn at.
  const HAND = 1.15; // a hand a touch larger than life, so the finger covers the cable button
  // Ripples small enough to stay inside the cable's box: the inner one hides under the finger.
  const RIPPLE = 0.75;
  const PRESS_TOP = 104; // room above the cable for the hand
  const cableY = PRESS_TOP;
  const buttonY = cableY + CABLE_BUTTON.y;
  const handOn = (p: { x: number; y: number }, k: number) => `translate(${p.x - HAND_TIP.x * k} ${p.y - HAND_TIP.y * k}) scale(${k})`;

  // plug: the tablet lies on its side (turned a quarter clockwise) so its bottom port faces the
  // cable's USB-C plug, which goes 8 units into the port.
  const PLUG_CABLE_Y = 82;
  const plugTip = { x: CABLE_USB_C_TIP.x - 8, y: PLUG_CABLE_Y + CABLE_USB_C_TIP.y };
  // rotate(90) maps (x, y) to (-y, x); solve for the translation that puts the port on the tip.
  const plugTablet = `translate(${plugTip.x + TABLET_PORT.y} ${plugTip.y - TABLET_PORT.x}) rotate(90)`;

  // power: the plug scene with a second hand, pointing left, on the power key (which faces right
  // once the tablet is on its side): both buttons held at once. The hold timer sits top left.
  const plugAt = { x: plugTip.x + TABLET_PORT.y, y: plugTip.y - TABLET_PORT.x };
  const powerEdge = { x: plugAt.x - TABLET_POWER_EDGE.y, y: plugAt.y + TABLET_POWER_EDGE.x };
  // rotate(90) maps the fingertip (48, 134) to (-134, 48).
  const powerHand = `translate(${powerEdge.x + HAND_TIP.y} ${powerEdge.y - HAND_TIP.x}) rotate(90)`;

  // The hold timer: the press glyph with its seconds set close underneath.
  const TIMER_TEXT_GAP = 44;
  const HOLD_TIMER = { x: VIEWBOX.cable.w + 24 + PRESS_POINT.x, y: buttonY };
  const POWER_TIMER = { x: PRESS_POINT.x, y: 40 };

  const BOXES: Record<CableSceneId, [number, number]> = {
    cable: [VIEWBOX.cable.w, VIEWBOX.cable.h],
    port: [200, 140],
    press: [VIEWBOX.cable.w, PRESS_TOP + VIEWBOX.cable.h],
    hold: [VIEWBOX.cable.w + 24 + VIEWBOX.press.w, buttonY + TIMER_TEXT_GAP + 8],
    plug: [424, 196],
    power: [556, 214],
  };
  const box = $derived(BOXES[scene]);
  const width = $derived(Math.round((height * box[0]) / box[1]));
</script>

{#snippet timer(at: { x: number; y: number })}
  <g transform="translate({at.x - PRESS_POINT.x} {at.y - PRESS_POINT.y})"><PressIndicator {title} /></g>
  {#if seconds !== undefined}
    <text data-part="seconds" x={at.x} y={at.y + TIMER_TEXT_GAP} text-anchor="middle" fill="currentColor"
      style="font: 600 18px var(--font-mono, monospace)">{seconds} s</text>
  {/if}
{/snippet}

<svg class="scene" role="img" aria-label={title} data-scene={scene} viewBox="0 0 {box[0]} {box[1]}" {width} {height}>
  {#if scene === 'cable'}
    <DeepFlashCable width={VIEWBOX.cable.w} highlight="button" {title} />
  {:else if scene === 'port'}
    <UsbAPort width={200} variant="adapterCrossed" highlight="port" {title} />
  {:else if scene === 'press' || scene === 'hold'}
    <g transform="translate(0 {cableY})"><DeepFlashCable width={VIEWBOX.cable.w} {title} /></g>
    {#if scene === 'press'}
      <g transform="translate({CABLE_BUTTON.x - PRESS_POINT.x * RIPPLE} {buttonY - PRESS_POINT.y * RIPPLE}) scale({RIPPLE})">
        <PressIndicator {title} />
      </g>
    {:else}
      {@render timer(HOLD_TIMER)}
    {/if}
    <g transform={handOn({ x: CABLE_BUTTON.x, y: buttonY }, HAND)}><Hand pose="press" {title} /></g>
  {:else if scene === 'plug' || scene === 'power'}
    <g transform={plugTablet}><Tablet screen="blank" highlight={scene === 'power' ? 'power' : undefined} {title} /></g>
    <g transform="translate(0 {PLUG_CABLE_Y})"><DeepFlashCable width={VIEWBOX.cable.w} {title} /></g>
    <g transform={handOn({ x: CABLE_BUTTON.x, y: PLUG_CABLE_Y + CABLE_BUTTON.y }, 1)}><Hand pose="press" {title} /></g>
    {#if scene === 'power'}
      <g transform={powerHand}><Hand pose="press" {title} /></g>
      {@render timer(POWER_TIMER)}
    {/if}
  {/if}
</svg>

<style>
  .scene {
    display: block;
    max-width: 100%;
    overflow: visible;
  }
</style>
