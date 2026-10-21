<script lang="ts">
  import { t } from "../lib/i18n";
  import { app, forgetRepo, openRepo, recentRepos } from "../lib/store.svelte";

  /** 打开仓库的下拉菜单：打开本地文件夹，或从最近打开的仓库里选 */
  let { x, y, onOpenFolder, onClose }: { x: number; y: number; onOpenFolder: () => void; onClose: () => void } =
    $props();

  let recent = $state(recentRepos());
  const openIds = $derived(new Set(app.tabs.map((t) => t.id)));
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>
<div class="dropdown" style="left:{Math.min(x, window.innerWidth - 440)}px; top:{y}px">
  <button
    class="item strong"
    onclick={() => {
      onClose();
      onOpenFolder();
    }}>{t("📂 打开本地仓库…")}</button
  >
  <button
    class="item strong"
    onclick={() => {
      onClose();
      app.cloneDialog = { mode: "clone" };
    }}>{t("⤓ 克隆远程仓库…")}</button
  >
  <button
    class="item strong"
    onclick={() => {
      onClose();
      app.cloneDialog = { mode: "init" };
    }}>{t("＋ 新建仓库…")}</button
  >
  {#if recent.length}<div class="sep-title">{t("最近打开")}</div>{/if}
  {#each recent as p (p)}
    <div class="row">
      <button
        class="item"
        onclick={() => {
          onClose();
          openRepo(p);
        }}
        title={p}
      >
        <span class="name">{p.split(/[\\/]/).filter(Boolean).pop()}{openIds.has(p) ? t(" · 已打开") : ""}</span>
        <span class="path">{p}</span>
      </button>
      <button
        class="x"
        title={t("从列表移除")}
        onclick={() => {
          forgetRepo(p);
          recent = recentRepos();
        }}>✕</button
      >
    </div>
  {/each}
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
  }
  .dropdown {
    position: fixed;
    z-index: 61;
    width: 430px;
    max-height: 70vh;
    overflow: auto;
    padding: 6px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }
  .row {
    display: flex;
    align-items: center;
  }
  .item {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    flex: 1;
    min-width: 0;
    height: auto;
    padding: 7px 10px;
    border: 0;
    background: none;
    text-align: left;
  }
  .item.strong {
    width: 100%;
    font-weight: 600;
  }
  .item:hover,
  .x:hover {
    background: var(--bg-hover);
  }
  .name {
    font-size: 13px;
  }
  .path {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .x {
    border: 0;
    background: none;
    color: var(--text-faint);
  }
  .sep-title {
    padding: 8px 10px 4px;
    color: var(--text-faint);
    font-size: 11.5px;
  }
</style>
