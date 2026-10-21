<script lang="ts">
  import { t } from "../lib/i18n";
  import type { RepoTab } from "../lib/store.svelte";
  import { isMac, relativeTime } from "../lib/util";
  import type { MenuItem } from "./ContextMenu.svelte";

  /** items：和多选右键菜单相同的操作 */
  let { tab, items }: { tab: RepoTab; items: MenuItem[] } = $props();

  const actions = $derived(items.filter((x) => x !== "sep"));
</script>

<div class="panel">
  <div class="head">
    <div class="kind">{t("多选")}</div>
    <div class="subject">{t("已选择 {0} 个提交", tab.picked.length)}</div>
    <div class="hint">{t("{0}点击加选或取消，Shift 点击选一段，Esc 取消选择", isMac ? "⌘ " : "Ctrl+")}</div>
    <div class="actions">
      {#each actions as a, i (a.label)}
        <button class:primary={i === 0} class:danger={a.danger} disabled={a.disabled} title={a.hint} onclick={a.action}>{a.label}</button>
      {/each}
    </div>
  </div>
  <div class="list">
    {#each tab.picked as p (p.oid)}
      <button class="item" class:cur={p.oid === tab.selectedOid} onclick={() => tab.scrollTo(p.row)}>
        <span class="sha">{p.oid.slice(0, 7)}</span>
        <span class="summary">{p.summary}</span>
        <span class="meta">{p.author} · {relativeTime(p.time)}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px 16px 12px;
    border-bottom: 1px solid var(--border);
  }
  .kind {
    color: var(--text-faint);
    font-size: 12px;
  }
  .subject {
    font-size: 15px;
    font-weight: 600;
  }
  .hint {
    color: var(--text-dim);
    font-size: 12px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 6px;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 0;
  }
  .item {
    display: grid;
    grid-template-columns: auto 1fr;
    column-gap: 8px;
    width: 100%;
    padding: 6px 16px;
    border: 0;
    border-radius: 0;
    background: none;
    text-align: left;
  }
  .item:hover {
    background: var(--bg-hover);
  }
  .item.cur {
    background: var(--bg-selected);
  }
  .sha {
    color: var(--text-faint);
    font-family: var(--mono);
    font-size: 12px;
  }
  .summary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    grid-column: 2;
    color: var(--text-dim);
    font-size: 12px;
  }
</style>
