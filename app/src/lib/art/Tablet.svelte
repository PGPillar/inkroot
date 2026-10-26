<script lang="ts">
  import DitherDefs from './DitherDefs.svelte';

  type Part = 'screen' | 'power' | 'port';
  type Screen = 'blank' | 'logo' | 'content' | 'prompt' | 'menu';
  let {
    width = 144,
    highlight,
    screen = 'blank',
    title,
  }: { width?: number; highlight?: Part; screen?: Screen; title: string } = $props();

  // Generic portrait slate, 3:4 body, deeper bottom bezel. Power on the top edge, USB-C on the bottom.
  const uid = $props.id();
  const pid = `${uid}-dither`;
  const fill = (part: Part) => (highlight === part ? `url(#${pid})` : 'none');
  const LINES = [64, 56, 62, 44, 64, 52, 60, 26];
  const ROWS = [48, 66, 84, 102, 120];
</script>

<svg role="img" aria-label={title} viewBox="0 0 144 200" {width} height={(width * 200) / 144}
  fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter">
  <DitherDefs id={pid} />
  <rect data-part="power" x="86" y="19" width="30" height="9" fill={fill('power')} />
  <path data-part="body" d="M58 178H21l-5-5V33l5-5h102l5 5v140l-5 5H86" />
  <path data-part="port" d="M58 178v-7h28v7" fill={fill('port')} />
  <rect data-part="screen" x="26" y="38" width="92" height="118" fill={fill('screen')} />
  {#if screen === 'logo'}
    <rect data-part="logo" x="62" y="93" width="20" height="8" />
  {:else if screen === 'content'}
    <path data-part="content"
      d={`M36 52h40${LINES.map((len, i) => `M36 ${68 + i * 10}h${len}`).join('')}`} />
  {:else if screen === 'prompt'}
    <g data-part="prompt">
      <rect x="36" y="70" width="72" height="56" style="fill: var(--panel)" />
      <path d="M46 82h30M62 97h34" />
      <rect x="46" y="92" width="10" height="10" />
      <path d="M48 96l3 3 4-5" stroke-linecap="butt" />
      <rect x="46" y="108" width="24" height="10" />
      <rect x="74" y="108" width="24" height="10" fill="currentColor" />
    </g>
  {:else if screen === 'menu'}
    <g data-part="menu">
      {#each ROWS as y, i}
        {#if i === 2}<rect x="27" y={y - 1} width="90" height="19" fill={`url(#${pid})`} stroke="none" />{/if}
        <rect x="34" y={y + 4} width="8" height="8" style={i === 2 ? 'fill: var(--panel)' : undefined} />
        {#if i === 2}<rect x="47" y={y + 3} width="42" height="10" style="fill: var(--panel)" stroke="none" />{/if}
        <path d={`M50 ${y + 8}h${i === 2 ? 36 : 44 - i * 4}`} />
        {#if i < ROWS.length - 1}<path d={`M27 ${y + 17}h90`} />{/if}
      {/each}
    </g>
  {/if}
  {#if highlight === 'power'}
    <path data-part="emphasis" d="M101 13V7M88 14l-4-4M114 14l4-4" />
  {:else if highlight === 'port'}
    <path data-part="emphasis" d="M72 184v6M59 183l-4 4M85 183l4 4" />
  {/if}
</svg>
