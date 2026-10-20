<script lang="ts">
  import { activate, app, closeTab, showTasks } from "../lib/store.svelte";
  import { isMac } from "../lib/util";
  import RepoPicker from "./RepoPicker.svelte";

  let { onOpenFolder }: { onOpenFolder: () => void } = $props();

  let picker = $state<{ x: number; y: number } | null>(null);
</script>

<!-- macOS 上标题栏和标签栏合在一起，左边给红绿灯按钮留位置；整条可以拖动窗口 -->
<div class="tabbar" class:mac={isMac} data-tauri-drag-region>
  <!-- 全局任务面板，固定在最左边，不属于任何仓库 -->
  <button class="pinned" class:active={app.view === "tasks"} onclick={showTasks} title="任务面板">
    ▦ 任务
    {#if app.tasks}
      {@const last = app.tasks.columns[app.tasks.columns.length - 1]?.id}
      {@const open = app.tasks.cards.filter((c) => c.column !== last && !c.archived).length}
      {#if open}<span class="pcount">{open}</span>{/if}
    {/if}
  </button>
  <span class="divider"></span>
  {#each app.tabs as t (t.key)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="tab"
      class:active={app.view === "repo" && t.key === app.activeKey}
      role="tab"
      tabindex="-1"
      aria-selected={t.key === app.activeKey}
      title={t.id || t.requestedPath}
      onclick={() => activate(t)}
      onauxclick={(e) => e.button === 1 && closeTab(t)}
    >
      <span class="ico">{t.loading ? "⟳" : t.error ? "⚠" : "⎇"}</span>
      <span class="name">{t.name}</span>
      {#if t.changeCount}<span class="dot" title="有未提交的更改"></span>{/if}
      <button
        class="close"
        title="关闭标签页"
        onclick={(e) => {
          e.stopPropagation();
          closeTab(t);
        }}>✕</button
      >
    </div>
  {/each}
  <button
    class="add"
    title="打开仓库"
    onclick={(e) => {
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      picker = { x: r.left, y: r.bottom + 4 };
    }}>＋</button
  >
  <div class="fill" data-tauri-drag-region></div>
</div>

{#if picker}
  <RepoPicker x={picker.x} y={picker.y} {onOpenFolder} onClose={() => (picker = null)} />
{/if}

<style>
  .tabbar {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 38px;
    padding: 0 8px;
    background: var(--bg-tabbar);
    border-bottom: 1px solid var(--border);
    user-select: none;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }
  .tabbar::-webkit-scrollbar {
    display: none;
  }
  .tabbar.mac {
    padding-left: 80px;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 31px;
    min-width: 120px;
    max-width: 230px;
    padding: 0 6px 0 12px;
    border: 1px solid transparent;
    border-bottom: 0;
    border-radius: 7px 7px 0 0;
    color: var(--text-dim);
    font-size: 12.5px;
    cursor: default;
    flex-shrink: 0;
  }
  .tab:hover {
    background: var(--bg-hover);
  }
  .tab.active {
    background: var(--bg-toolbar);
    border-color: var(--border);
    color: var(--text);
    margin-bottom: -1px;
    height: 32px;
  }
  .ico {
    font-size: 12px;
    color: var(--accent);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--warn);
    flex-shrink: 0;
  }
  .close {
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--text-faint);
    font-size: 10px;
    visibility: hidden;
  }
  .tab:hover .close,
  .tab.active .close {
    visibility: visible;
  }
  .close:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .pinned {
    display: flex;
    align-items: center;
    gap: 6px;
    align-self: flex-end;
    height: 31px;
    padding: 0 12px;
    border: 1px solid transparent;
    border-bottom: 0;
    border-radius: 7px 7px 0 0;
    background: none;
    color: var(--text-dim);
    font-size: 12.5px;
    font-weight: 600;
    flex-shrink: 0;
  }
  .pinned:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .pinned.active {
    background: var(--bg-toolbar);
    border-color: var(--border);
    color: var(--text);
    margin-bottom: -1px;
    height: 32px;
  }
  .pcount {
    padding: 0 6px;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-size: 10.5px;
    line-height: 15px;
  }
  .divider {
    align-self: center;
    width: 1px;
    height: 18px;
    margin: 0 4px;
    background: var(--border);
    flex-shrink: 0;
  }
  .add {
    align-self: center;
    width: 28px;
    height: 26px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-dim);
    font-size: 15px;
    flex-shrink: 0;
  }
  .add:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .fill {
    flex: 1;
    align-self: stretch;
    min-width: 40px;
  }
</style>
