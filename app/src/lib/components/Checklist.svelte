<script lang="ts">
  let { items }: { items: { label: string; done: boolean }[] } = $props();
</script>

<ul class="checks">
  {#each items as item, i (i)}
    <li class:done={item.done} data-done={item.done}>
      <span class="box" aria-hidden="true"></span>
      <span>{item.label}</span>
    </li>
  {/each}
</ul>

<style>
  .checks {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 7px;
    font: var(--fs-body);
  }
  li {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .box {
    flex: none;
    box-sizing: border-box;
    width: 14px;
    height: 14px;
    border: var(--border);
    border-radius: 0;
    display: grid;
    place-items: center;
  }
  .done .box::after {
    content: '';
    width: 8px;
    height: 8px;
    background: var(--ink);
  }
</style>
