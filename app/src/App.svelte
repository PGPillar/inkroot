<script lang="ts">
  import { onMount } from 'svelte';
  import Shell from './lib/Shell.svelte';
  import { selectBackend } from './lib/backend';
  import { setApp } from './lib/context';
  import { START_FAILED, STARTING } from './lib/copy';
  import { AppState } from './lib/state.svelte';

  // Context can only be set while the component initialises, so the state is built here
  // and only the connection waits for mount.
  const { backend, platform } = selectBackend(location, window);
  const app = new AppState(backend, platform);
  setApp(app);
  let failed = $state(false);

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let gone = false;
    app.connect().then((u) => (gone ? u() : (unlisten = u)), (e) => {
      console.error('Inkroot could not connect', e);
      failed = true;
    });
    return () => {
      gone = true;
      unlisten?.();
    };
  });
</script>

{#if app.info}
  <Shell />
{:else}
  <p class="starting" role={failed ? 'alert' : undefined}>{failed ? START_FAILED : STARTING}</p>
{/if}

<style>
  .starting {
    height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    font: var(--fs-meta);
  }
</style>
