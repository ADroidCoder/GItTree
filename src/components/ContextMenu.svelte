<script lang="ts" module>
  export type MenuItem =
    | { label: string; action: () => void; danger?: boolean; disabled?: boolean; hint?: string }
    | "sep";
</script>

<script lang="ts">
  let { x, y, items, onClose }: { x: number; y: number; items: MenuItem[]; onClose: () => void } = $props();

  let el = $state<HTMLDivElement>();
  // 菜单超出窗口时往回挪
  const pos = $derived.by(() => {
    const w = el?.offsetWidth ?? 220;
    const h = el?.offsetHeight ?? items.length * 28;
    return {
      left: Math.min(x, window.innerWidth - w - 6),
      top: Math.min(y, window.innerHeight - h - 6),
    };
  });
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} onblur={onClose} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose} oncontextmenu={(e) => { e.preventDefault(); onClose(); }}></div>
<div class="menu" bind:this={el} style="left:{pos.left}px; top:{pos.top}px" role="menu">
  {#each items as it, i (i)}
    {#if it === "sep"}
      <div class="sep"></div>
    {:else}
      <button
        class="mi"
        class:danger={it.danger}
        disabled={it.disabled}
        role="menuitem"
        onclick={() => {
          onClose();
          it.action();
        }}
      >
        <span>{it.label}</span>
        {#if it.hint}<span class="hint">{it.hint}</span>{/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
  }
  .menu {
    position: fixed;
    z-index: 51;
    min-width: 220px;
    max-width: 420px;
    padding: 5px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }
  .mi {
    display: flex;
    justify-content: space-between;
    gap: 20px;
    width: 100%;
    height: 28px;
    padding: 0 10px;
    border: 0;
    border-radius: 4px;
    background: none;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mi:hover:not(:disabled) {
    background: var(--accent);
    color: #fff;
  }
  .mi.danger {
    color: var(--del);
  }
  .mi.danger:hover:not(:disabled) {
    background: var(--del);
    color: #fff;
  }
  .hint {
    color: var(--text-faint);
    font-family: var(--mono);
    font-size: 12px;
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
</style>
