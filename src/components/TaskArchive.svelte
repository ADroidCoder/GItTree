<script lang="ts">
  import { t } from "../lib/i18n";
  import { unarchive, type Board, type Card } from "../lib/board";
  import { confirm, saveTasksSoon } from "../lib/store.svelte";
  import { avatarColor, initials } from "../lib/util";

  let {
    board,
    filter,
    assigneeFilter,
    repoFilter,
    onOpen,
    setAutoArchive,
  }: {
    board: Board;
    filter: string;
    assigneeFilter: string;
    repoFilter: string;
    onOpen: (c: Card) => void;
    setAutoArchive: (days: number) => void;
  } = $props();

  /** 归档可能越积越多，一次只渲染这么多行 */
  const PAGE = 200;
  let shown = $state(PAGE);

  const repoName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  const columnName = (id: string) => board.columns.find((c) => c.id === id)?.name ?? t("已删除的列");

  /** 归档的任务，最近归档的在前，和看板用同一套筛选 */
  const cards = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    return board.cards
      .filter((c) => c.archived)
      .filter((c) => !assigneeFilter || c.assignee === assigneeFilter)
      .filter((c) => !repoFilter || (repoFilter === "-" ? !c.repo : c.repo === repoFilter))
      .filter((c) => !f || `${c.title} ${c.description} ${c.branch} ${c.assignee}`.toLowerCase().includes(f))
      .sort((a, b) => b.archived - a.archived);
  });

  /** 按归档月份分组 */
  const groups = $derived.by(() => {
    const out: { month: string; cards: Card[] }[] = [];
    for (const c of cards.slice(0, shown)) {
      const d = new Date(c.archived);
      const month = t("{0} 年 {1} 月", d.getFullYear(), d.getMonth() + 1);
      if (out[out.length - 1]?.month !== month) out.push({ month, cards: [] });
      out[out.length - 1].cards.push(c);
    }
    return out;
  });

  const date = (ms: number) => {
    const d = new Date(ms);
    return `${d.getMonth() + 1}-${String(d.getDate()).padStart(2, "0")}`;
  };

  function restore(c: Card) {
    unarchive(board, c);
    saveTasksSoon();
  }

  async function remove(c: Card) {
    if (!(await confirm(t("删除任务"), t("确定永久删除 “{0}” 吗？", c.title || t("（无标题）")), t("删除"), true))) return;
    board.cards = board.cards.filter((x) => x.id !== c.id);
    saveTasksSoon();
  }

  async function removeAll() {
    if (!(await confirm(t("删除归档"), t("永久删除下面列出的 {0} 个归档任务？删除后不能恢复。", cards.length), t("全部删除"), true))) return;
    const ids = new Set(cards.map((c) => c.id));
    board.cards = board.cards.filter((c) => !ids.has(c.id));
    saveTasksSoon();
  }
</script>

<div class="archive">
  <div class="head">
    <span class="title">{t("归档")}</span>
    <span class="count">{t("{0} 个任务", cards.length)}</span>
    <span class="grow"></span>
    <label for="auto-archive">{t("完成后自动归档")}</label>
    <select id="auto-archive" value={board.autoArchiveDays} onchange={(e) => setAutoArchive(Number(e.currentTarget.value))}>
      <option value={1}>{t("1 天后")}</option>
      <option value={3}>{t("3 天后")}</option>
      <option value={7}>{t("7 天后")}</option>
      <option value={14}>{t("14 天后")}</option>
      <option value={30}>{t("30 天后")}</option>
      <option value={0}>{t("不自动归档")}</option>
    </select>
    {#if cards.length}<button class="danger" onclick={removeAll}>{t("全部删除")}</button>{/if}
  </div>

  <div class="list">
    {#if cards.length === 0}
      <div class="empty">
        {filter || assigneeFilter || repoFilter ? t("没有符合筛选条件的归档任务") : t("还没有归档的任务。已完成列里的任务可以手动归档，也会按上面的设置自动归档。")}
      </div>
    {/if}
    {#each groups as g (g.month)}
      <div class="month">{g.month}</div>
      {#each g.cards as c (c.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="row" onclick={() => onOpen(c)} title={t("点击查看详情")}>
          <span class="date">{date(c.archived)}</span>
          <span class="name">{c.title || t("（无标题）")}</span>
          <span class="col">{columnName(c.column)}</span>
          {#if c.repo}<span class="tag repo" title={c.repo}>▣ {repoName(c.repo)}</span>{/if}
          {#if c.branch}<span class="tag branch" title={c.branch}>⑂ {c.branch}</span>{/if}
          {#if c.assignee}
            <span class="avatar" style="background:{avatarColor(c.assignee)}" title={c.assignee}>{initials(c.assignee)}</span>
          {/if}
          <span class="acts">
            <button onclick={(e) => (e.stopPropagation(), restore(c))} title={t("放回看板")}>{t("恢复")}</button>
            <button onclick={(e) => (e.stopPropagation(), remove(c))} title={t("永久删除")}>{t("删除")}</button>
          </span>
        </div>
      {/each}
    {/each}
    {#if cards.length > shown}
      <button class="more" onclick={() => (shown += PAGE)}>{t("再显示 {0} 个（还有 {1} 个）", Math.min(PAGE, cards.length - shown), cards.length - shown)}</button>
    {/if}
  </div>
</div>

<style>
  .archive {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 14px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 10px;
    font-size: 13px;
  }
  .title {
    font-weight: 600;
  }
  .count,
  label {
    color: var(--text-dim);
  }
  .grow {
    flex: 1;
  }
  select {
    height: 28px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-side);
  }
  .empty {
    padding: 40px;
    color: var(--text-faint);
    text-align: center;
  }
  .month {
    position: sticky;
    top: 0;
    padding: 8px 14px 4px;
    background: var(--bg-side);
    color: var(--text-dim);
    font-size: 11.5px;
    font-weight: 700;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px 0 14px;
    font-size: 13px;
    cursor: default;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .date {
    flex-shrink: 0;
    width: 40px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .col {
    flex-shrink: 0;
    color: var(--text-faint);
    font-size: 12px;
  }
  .tag {
    max-width: 140px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    padding: 0 6px;
    border-radius: 3px;
    font-size: 11.5px;
  }
  .repo {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .branch {
    background: color-mix(in srgb, var(--add) 18%, transparent);
    font-family: var(--mono);
  }
  .avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    color: #fff;
    font-size: 10.5px;
    font-weight: 600;
  }
  .acts {
    display: flex;
    gap: 4px;
    opacity: 0;
  }
  .row:hover .acts {
    opacity: 1;
  }
  .acts button {
    height: 24px;
    padding: 0 8px;
    font-size: 12px;
  }
  .more {
    display: block;
    width: calc(100% - 20px);
    margin: 8px 10px;
  }
</style>
