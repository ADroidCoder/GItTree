<script lang="ts">
  import { t } from "../lib/i18n";
  import { PRIORITIES, autoArchive, markDone, uid, type Card, type Column } from "../lib/board";
  import { app, confirm, dialog, openSettings, saveTasksSoon, toast } from "../lib/store.svelte";
  import { avatarColor, initials } from "../lib/util";
  import TaskArchive from "./TaskArchive.svelte";
  import TaskEditor from "./TaskEditor.svelte";

  /** 已完成列默认只显示最近这么多个，其余的折叠起来 */
  const DONE_LIMIT = 20;

  /** 全局任务面板：任务不属于任何仓库，只是可以选择关联一个仓库和分支 */
  const board = $derived(app.tasks);
  let repoFilter = $state("");
  let filter = $state("");
  let assigneeFilter = $state("");
  let editing = $state<Card | null>(null);
  let dragging = $state<string | null>(null);
  let dropAt = $state<{ col: string; index: number } | null>(null);
  let showArchive = $state(false);
  /** 已完成列是否展开显示全部 */
  let showAllDone = $state(false);

  const assignees = $derived([...new Set((board?.cards ?? []).map((c) => c.assignee).filter(Boolean))].sort());
  const repos = $derived([...new Set((board?.cards ?? []).map((c) => c.repo).filter(Boolean))].sort());
  const repoName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  /** 关联分支是否还存在：只有仓库已经在标签页里打开时才知道 */
  function branchState(c: Card): "ok" | "gone" | "unknown" {
    const t = app.tabs.find((x) => x.id === c.repo);
    if (!t?.repo) return "unknown";
    return t.repo.refs.some((r) => r.kind === "local" && r.name === c.branch) ? "ok" : "gone";
  }
  const today = new Date().toISOString().slice(0, 10);

  function cardsOf(col: string): Card[] {
    const f = filter.trim().toLowerCase();
    return (board?.cards ?? [])
      .filter((c) => c.column === col && !c.archived)
      .filter((c) => !assigneeFilter || c.assignee === assigneeFilter)
      .filter((c) => !repoFilter || (repoFilter === "-" ? !c.repo : c.repo === repoFilter))
      .filter((c) => !f || `${c.title} ${c.description} ${c.branch} ${c.assignee}`.toLowerCase().includes(f))
      .sort((a, b) => a.order - b.order);
  }

  function newCard(col: string): Card {
    const now = Date.now();
    return {
      id: uid(),
      column: col,
      title: "",
      description: "",
      assignee: assigneeFilter,
      priority: "",
      due: "",
      repo: repoFilter && repoFilter !== "-" ? repoFilter : "",
      branch: "",
      checklist: [],
      order: -now, // 新任务放在列的最上面
      created: now,
      updated: now,
      doneAt: 0,
      archived: 0,
    };
  }

  function saveCard(c: Card) {
    if (!board) return;
    c.updated = Date.now();
    markDone(board, c);
    const i = board.cards.findIndex((x) => x.id === c.id);
    if (i >= 0) board.cards[i] = c;
    else board.cards.push(c);
    saveTasksSoon();
  }

  function deleteCard(id: string) {
    if (!board) return;
    board.cards = board.cards.filter((c) => c.id !== id);
    saveTasksSoon();
  }

  function archiveCard(id: string) {
    const c = board?.cards.find((x) => x.id === id);
    if (!c) return;
    c.archived = Date.now();
    saveTasksSoon();
    toast(t("已归档 “{0}”，可以在「归档」里恢复", c.title));
  }

  /** 把一列里（按当前筛选能看到的）任务全部归档 */
  async function archiveColumn(col: Column) {
    const cards = cardsOf(col.id);
    if (!cards.length) return;
    if (!(await confirm(t("归档任务"), t("把 “{0}” 里的 {1} 个任务全部归档？归档后不在看板上显示，可以在「归档」里搜索和恢复。", col.name, cards.length), t("归档")))) return;
    const now = Date.now();
    for (const c of cards) c.archived = now;
    saveTasksSoon();
  }

  function setAutoArchive(days: number) {
    if (!board) return;
    board.autoArchiveDays = days;
    const n = autoArchive(board);
    saveTasksSoon();
    if (n) toast(t("已自动归档 {0} 个完成超过 {1} 天的任务", n, days));
  }

  // ---------- 列 ----------
  async function addColumn() {
    const r = await dialog({ title: t("添加列"), input: { placeholder: t("列名，例如 待发布"), value: "" }, confirmText: t("添加") });
    if (!board || !r?.input.trim()) return;
    board.columns.push({ id: uid(), name: r.input.trim() });
    saveTasksSoon();
  }

  async function renameColumn(col: Column) {
    const r = await dialog({ title: t("重命名列"), input: { placeholder: t("列名"), value: col.name }, confirmText: t("保存") });
    if (!r?.input.trim()) return;
    col.name = r.input.trim();
    saveTasksSoon();
  }

  async function deleteColumn(col: Column) {
    if (!board) return;
    const n = board.cards.filter((c) => c.column === col.id && !c.archived).length;
    if (n > 0) {
      if (!(await confirm(t("删除列"), t("“{0}” 里还有 {1} 个任务，删除列会同时删除这些任务。", col.name, n), t("删除"), true))) return;
      board.cards = board.cards.filter((c) => c.column !== col.id || c.archived);
    }
    board.columns = board.columns.filter((c) => c.id !== col.id);
    saveTasksSoon();
  }

  function moveColumn(col: Column, dir: -1 | 1) {
    if (!board) return;
    const i = board.columns.indexOf(col);
    const j = i + dir;
    if (j < 0 || j >= board.columns.length) return;
    [board.columns[i], board.columns[j]] = [board.columns[j], board.columns[i]];
    saveTasksSoon();
  }

  // ---------- 拖动卡片 ----------
  function onDragOver(e: DragEvent, col: string, list: HTMLElement) {
    if (!dragging) return;
    e.preventDefault();
    // 根据鼠标位置算出要插到第几个卡片前面
    const cards = [...list.querySelectorAll<HTMLElement>("[data-card]")].filter((el) => el.dataset.card !== dragging);
    let index = cards.length;
    for (let i = 0; i < cards.length; i++) {
      const r = cards[i].getBoundingClientRect();
      if (e.clientY < r.top + r.height / 2) {
        index = i;
        break;
      }
    }
    dropAt = { col, index };
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    if (!board || !dragging || !dropAt) return;
    const card = board.cards.find((c) => c.id === dragging);
    if (card) {
      const target = cardsOf(dropAt.col).filter((c) => c.id !== card.id);
      target.splice(dropAt.index, 0, card);
      card.column = dropAt.col;
      card.updated = Date.now();
      markDone(board, card);
      target.forEach((c, i) => (c.order = i));
      saveTasksSoon();
    }
    dragging = null;
    dropAt = null;
  }

  function priorityColor(p: string) {
    return PRIORITIES.find((x) => x.value === p)?.color ?? "transparent";
  }

  const lastColumn = $derived(board?.columns[board.columns.length - 1]?.id);
  const archivedCount = $derived((board?.cards ?? []).filter((c) => c.archived).length);
</script>

<div class="board-view">
  <div class="bar">
    <div class="title">{t("任务")}</div>
    <input class="search" placeholder={t("搜索任务")} bind:value={filter} />
    <select bind:value={assigneeFilter} title={t("按负责人筛选")}>
      <option value="">{t("全部负责人")}</option>
      {#each assignees as a (a)}<option value={a}>{a}</option>{/each}
    </select>
    <select bind:value={repoFilter} title={t("按关联仓库筛选")}>
      <option value="">{t("全部仓库")}</option>
      <option value="-">{t("未关联仓库")}</option>
      {#each repos as r (r)}<option value={r}>{repoName(r)}</option>{/each}
    </select>
    <span class="grow"></span>
    <button onclick={() => openSettings("notify")} title={t("定时提醒、任务到期提醒等设置")}>{t("⏰ 提醒")}</button>
    <button class:on={showArchive} onclick={() => (showArchive = !showArchive)} title={t("查看、搜索和恢复归档的任务")}>
      {t("▤ 归档")}{#if archivedCount}<span class="acount">{archivedCount}</span>{/if}
    </button>
    {#if !showArchive}<button onclick={addColumn}>{t("＋ 添加列")}</button>{/if}
    <button class="primary" onclick={() => board && (editing = newCard(board.columns[0]?.id ?? "todo"))}>{t("＋ 新任务")}</button>
  </div>

  {#if !board}
    <div class="loading">{t("加载中…")}</div>
  {:else if showArchive}
    <TaskArchive {board} {filter} {assigneeFilter} {repoFilter} onOpen={(c) => (editing = { ...c })} {setAutoArchive} />
  {:else}
    <div class="columns">
      {#each board.columns as col, ci (col.id)}
        {@const all = cardsOf(col.id)}
        {@const isDone = col.id === lastColumn}
        {@const cards = isDone && !showAllDone ? all.slice(0, DONE_LIMIT) : all}
        <div class="column">
          <div class="col-head">
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="col-name" ondblclick={() => renameColumn(col)} title={t("双击重命名")}>{col.name}</span>
            <span class="count">{all.length}</span>
            <span class="grow"></span>
            <span class="col-acts">
              <button title={t("左移")} disabled={ci === 0} onclick={() => moveColumn(col, -1)}>‹</button>
              <button title={t("右移")} disabled={ci === board.columns.length - 1} onclick={() => moveColumn(col, 1)}>›</button>
              <button title={t("重命名")} onclick={() => renameColumn(col)}>✎</button>
              <button title={t("全部归档")} disabled={!all.length} onclick={() => archiveColumn(col)}>▤</button>
              <button title={t("删除列")} onclick={() => deleteColumn(col)}>✕</button>
            </span>
            <button class="add" title={t("在这一列添加任务")} onclick={() => (editing = newCard(col.id))}>＋</button>
          </div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="list"
            class:drop={dropAt?.col === col.id}
            ondragover={(e) => onDragOver(e, col.id, e.currentTarget as HTMLElement)}
            ondragleave={(e) => {
              if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) dropAt = null;
            }}
            ondrop={onDrop}
          >
            {#each cards.filter((c) => c.id !== dragging) as c, i (c.id)}
              {#if dropAt?.col === col.id && dropAt.index === i}<div class="placeholder"></div>{/if}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                class="card"
                data-card={c.id}
                draggable="true"
                role="button"
                tabindex="0"
                style="--p:{priorityColor(c.priority)}"
                ondragstart={(e) => {
                  e.dataTransfer?.setData("text/plain", c.id);
                  // 等浏览器生成拖动图像后再隐藏原卡片
                  setTimeout(() => (dragging = c.id));
                }}
                ondragend={() => {
                  dragging = null;
                  dropAt = null;
                }}
                onclick={() => (editing = { ...c })}
              >
                {#if isDone}
                  <button class="card-archive" title={t("归档")} onclick={(e) => (e.stopPropagation(), archiveCard(c.id))}>▤</button>
                {/if}
                <div class="card-title" class:done={isDone}>{c.title || t("（无标题）")}</div>
                {#if c.description}<div class="card-desc">{c.description}</div>{/if}
                <div class="card-meta">
                  {#if c.priority}<span class="prio">{PRIORITIES.find((p) => p.value === c.priority)?.label}</span>{/if}
                  {#if c.checklist.length}
                    {@const n = c.checklist.filter((x) => x.done).length}
                    <span class="check" class:complete={n === c.checklist.length} title={t("检查项")}>☑ {n}/{c.checklist.length}</span>
                  {/if}
                  {#if c.due}
                    <span class="due" class:overdue={c.due < today && col.id !== lastColumn}>📅 {c.due.slice(5)}</span>
                  {/if}
                  {#if c.repo}
                    <span class="repo" title={c.repo}>▣ {repoName(c.repo)}</span>
                  {/if}
                  {#if c.branch}
                    {@const bs = branchState(c)}
                    <span class="branch" class:gone={bs === "gone"} title={bs === "gone" ? t("{0}（本地已不存在，可能已合并删除）", c.branch) : c.branch}>
                      ⑂ {c.branch}
                    </span>
                  {/if}
                  <span class="grow"></span>
                  {#if c.assignee}
                    <span class="avatar" style="background:{avatarColor(c.assignee)}" title={c.assignee}>{initials(c.assignee)}</span>
                  {/if}
                </div>
              </div>
            {/each}
            {#if dropAt?.col === col.id && dropAt.index >= cards.filter((c) => c.id !== dragging).length}
              <div class="placeholder"></div>
            {/if}
            {#if isDone && all.length > DONE_LIMIT}
              <div class="more">
                {#if showAllDone}
                  <button onclick={() => (showAllDone = false)}>{t("只显示最近 {0} 个", DONE_LIMIT)}</button>
                {:else}
                  <button onclick={() => (showAllDone = true)}>{t("还有 {0} 个，全部显示", all.length - DONE_LIMIT)}</button>
                {/if}
                <button onclick={() => archiveColumn(col)}>{t("全部归档")}</button>
              </div>
            {/if}
            {#if isDone && board.autoArchiveDays && all.length}
              <div class="auto-hint">{t("完成 {0} 天后自动归档", board.autoArchiveDays)}</div>
            {/if}
            {#if all.length === 0 && !dropAt}
              <button class="empty" onclick={() => (editing = newCard(col.id))}>{t("＋ 添加任务")}</button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if editing && board}
  <TaskEditor
    card={editing}
    columns={board.columns}
    {assignees}
    isNew={!board.cards.some((c) => c.id === editing!.id)}
    onSave={(c) => {
      saveCard(c);
      editing = null;
    }}
    onDelete={(id) => {
      deleteCard(id);
      editing = null;
    }}
    onArchive={(id) => {
      archiveCard(id);
      editing = null;
    }}
    onClose={() => (editing = null)}
  />
{/if}

<style>
  .board-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-graph);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 46px;
    padding: 0 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .title {
    font-weight: 600;
    margin-right: 8px;
  }
  .search {
    width: 200px;
  }
  select {
    height: 30px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
  }
  .grow {
    flex: 1;
  }
  .loading {
    margin: auto;
    color: var(--text-dim);
  }
  .columns {
    flex: 1;
    display: flex;
    gap: 12px;
    padding: 14px;
    overflow-x: auto;
    min-height: 0;
  }
  .column {
    display: flex;
    flex-direction: column;
    width: 290px;
    flex-shrink: 0;
    min-height: 0;
    background: var(--bg-side);
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .col-head {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 40px;
    padding: 0 8px 0 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .col-name {
    font-weight: 600;
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .count {
    padding: 0 7px;
    border-radius: 9px;
    background: var(--bg-hover);
    color: var(--text-dim);
    font-size: 11.5px;
  }
  .col-acts {
    display: none;
    gap: 2px;
  }
  .col-head:hover .col-acts {
    display: flex;
  }
  .col-acts button,
  .add {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-dim);
  }
  .col-acts button:hover:not(:disabled),
  .add:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .list {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    overflow-y: auto;
    min-height: 60px;
  }
  .list.drop {
    background: var(--bg-hover);
  }
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px 10px 14px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-card);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
    cursor: grab;
    overflow: hidden;
  }
  .card::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 4px;
    background: var(--p);
  }
  .card:hover {
    border-color: var(--accent);
  }
  .card-title {
    font-size: 13px;
    font-weight: 600;
    line-height: 1.4;
    word-break: break-word;
  }
  .card-title.done {
    color: var(--text-dim);
    text-decoration: line-through;
  }
  .card-desc {
    color: var(--text-dim);
    font-size: 12px;
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    white-space: pre-wrap;
  }
  .card-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    font-size: 11.5px;
    color: var(--text-dim);
  }
  .prio {
    padding: 0 6px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--p) 25%, transparent);
    color: var(--text);
  }
  .due.overdue {
    color: var(--del);
    font-weight: 600;
  }
  .branch {
    max-width: 150px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0 6px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--add) 18%, transparent);
    font-family: var(--mono);
  }
  .repo {
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0 6px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .branch.gone {
    background: var(--bg-hover);
    text-decoration: line-through;
  }
  .avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    color: #fff;
    font-size: 10.5px;
    font-weight: 600;
  }
  .bar button.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .acount {
    margin-left: 5px;
    padding: 0 6px;
    border-radius: 8px;
    background: var(--bg-hover);
    font-size: 11px;
  }
  .card-archive {
    position: absolute;
    top: 6px;
    right: 6px;
    display: none;
    width: 22px;
    height: 22px;
    padding: 0;
    border: 0;
    background: var(--bg-hover);
    color: var(--text-dim);
  }
  .card:hover .card-archive {
    display: block;
  }
  .card-archive:hover {
    color: var(--text);
  }
  .check {
    padding: 0 6px;
    border-radius: 3px;
    background: var(--bg-hover);
  }
  .check.complete {
    background: color-mix(in srgb, var(--add) 25%, transparent);
    color: var(--text);
  }
  .more {
    display: flex;
    gap: 6px;
  }
  .more button {
    flex: 1;
    height: 28px;
    font-size: 12px;
  }
  .auto-hint {
    color: var(--text-faint);
    font-size: 11.5px;
    text-align: center;
  }
  .placeholder {
    height: 46px;
    border: 2px dashed var(--accent);
    border-radius: 6px;
    flex-shrink: 0;
  }
  .empty {
    border: 1px dashed var(--border);
    background: none;
    color: var(--text-faint);
    height: 40px;
  }
</style>
