<script lang="ts">
  import { t } from "../lib/i18n";
  import { PRIORITIES, branchSlug, uid, type Card, type Column } from "../lib/board";
  import { activate, app, confirm, openRepo, recentRepos } from "../lib/store.svelte";

  let {
    card,
    columns,
    assignees,
    isNew,
    onSave,
    onDelete,
    onArchive,
    onClose,
  }: {
    card: Card;
    columns: Column[];
    assignees: string[];
    isNew: boolean;
    onSave: (c: Card) => void;
    onDelete: (id: string) => void;
    onArchive: (id: string) => void;
    onClose: () => void;
  } = $props();

  // 在副本上编辑，点保存才写回
  // svelte-ignore state_referenced_locally
  let c = $state<Card>({ ...card, checklist: card.checklist.map((x) => ({ ...x })) });
  let newItem = $state("");
  const checked = $derived(c.checklist.filter((x) => x.done).length);

  function addItem() {
    const text = newItem.trim();
    if (!text) return;
    c.checklist.push({ id: uid(), text, done: false });
    newItem = "";
  }
  let titleEl = $state<HTMLInputElement>();
  let creating = $state(false);

  $effect(() => {
    titleEl?.focus();
  });

  /** 可以关联的仓库：已打开的标签页 + 最近打开过的 */
  const repoOptions = $derived(
    [...new Set([...app.tabs.map((t) => t.id).filter(Boolean), ...recentRepos(), c.repo].filter(Boolean))].sort(),
  );
  const repoName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  /** 关联的仓库如果已经在标签页里打开，就能列出它的分支 */
  const repoTab = $derived(app.tabs.find((t) => t.id === c.repo && t.repo));
  const localBranches = $derived((repoTab?.repo?.refs ?? []).filter((r) => r.kind === "local").map((r) => r.name).sort());
  const branchExists = $derived(!!c.branch && localBranches.includes(c.branch));
  const flowReady = $derived(!!repoTab?.flow?.initialized);

  function save() {
    if (!c.title.trim()) return titleEl?.focus();
    c.title = c.title.trim();
    c.checklist = c.checklist.filter((x) => x.text.trim());
    onSave({ ...c });
  }

  /** 可选操作：在关联的仓库里为任务创建分支（初始化了 Git Flow 就开始一个 feature） */
  async function createBranch() {
    const tb = repoTab;
    if (!tb || !c.title.trim()) return titleEl?.focus();
    const slug = branchSlug(c);
    creating = true;
    const name = flowReady ? `${tb.flow!.feature}${slug}` : slug;
    const ok = flowReady
      ? await tb.run({ op: "flow_start", kind: "feature", name: slug, base: null }, t("正在创建 {0}…", name))
      : await tb.run({ op: "create_branch", name, start: "HEAD", checkout: true }, t("正在创建 {0}…", name));
    creating = false;
    if (ok) c.branch = name;
  }

  async function gotoRepo() {
    save();
    await openRepo(c.repo);
    const tb = app.tabs.find((x) => x.id === c.repo);
    const ref = tb?.repo?.refs.find((r) => r.kind === "local" && r.name === c.branch);
    if (tb && ref?.row != null) {
      activate(tb);
      tb.select(ref.row, ref.oid);
      tb.scrollTo(ref.row);
    }
  }

  /** 和归档页的“恢复”一样：原来的列不在了就放到最后一列，完成时间重新算 */
  function restore() {
    c.archived = 0;
    c.doneAt = 0;
    if (!columns.some((x) => x.id === c.column)) c.column = columns[columns.length - 1]?.id ?? c.column;
    save();
  }

  async function remove() {
    if (await confirm(t("删除任务"), t("确定删除 “{0}” 吗？", c.title || t("（无标题）")), t("删除"), true)) onDelete(c.id);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) save();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={onKey}>
  <div class="dialog" role="dialog" aria-modal="true">
    <input
      class="title-input"
      bind:this={titleEl}
      bind:value={c.title}
      placeholder={t("任务标题")}
      onkeydown={(e) => e.key === "Enter" && !e.isComposing && save()}
    />
    {#if c.archived}
      <div class="archived">{t("这个任务已经归档，不在看板上显示。保存修改不会恢复，点下面的「恢复到看板」才会放回去。")}</div>
    {/if}
    <textarea bind:value={c.description} rows="5" placeholder={t("描述（可选）")}></textarea>

    <div class="checklist">
      <div class="ck-head">
        <span>{t("检查项")}</span>
        {#if c.checklist.length}
          <span class="ck-count">{checked}/{c.checklist.length}</span>
          <span class="ck-bar"><span style="width:{(checked / c.checklist.length) * 100}%"></span></span>
        {/if}
      </div>
      {#each c.checklist as item, i (item.id)}
        <div class="ck-item">
          <input type="checkbox" bind:checked={item.done} />
          <input class="ck-text" class:done={item.done} bind:value={item.text} onkeydown={(e) => e.key === "Enter" && e.preventDefault()} />
          <button class="ck-del" title={t("删除")} onclick={() => c.checklist.splice(i, 1)}>✕</button>
        </div>
      {/each}
      <input
        class="ck-new"
        bind:value={newItem}
        placeholder={t("＋ 添加检查项，回车确认")}
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.isComposing && !e.metaKey && !e.ctrlKey) {
            e.preventDefault();
            addItem();
          }
        }}
      />
    </div>

    <div class="grid">
      <label for="t-col">{t("状态")}</label>
      <select id="t-col" bind:value={c.column}>
        {#each columns as col (col.id)}<option value={col.id}>{col.name}</option>{/each}
      </select>

      <label for="t-who">{t("负责人")}</label>
      <input id="t-who" bind:value={c.assignee} list="t-assignees" placeholder={t("姓名")} />
      <datalist id="t-assignees">
        {#each assignees as a (a)}<option value={a}></option>{/each}
      </datalist>

      <span class="label">{t("优先级")}</span>
      <div class="prios">
        {#each PRIORITIES as p (p.value)}
          <button class="prio" class:on={c.priority === p.value} style="--p:{p.color}" onclick={() => (c.priority = p.value)}>
            {p.label}
          </button>
        {/each}
      </div>

      <label for="t-due">{t("截止日期")}</label>
      <div class="row">
        <input id="t-due" type="date" bind:value={c.due} />
        {#if c.due}<button class="small" onclick={() => (c.due = "")}>{t("清除")}</button>{/if}
      </div>

      <label for="t-repo">{t("关联仓库")}</label>
      <div class="row">
        <select id="t-repo" bind:value={c.repo}>
          <option value="">{t("不关联")}</option>
          {#each repoOptions as r (r)}<option value={r}>{repoName(r)} — {r}</option>{/each}
        </select>
        {#if c.repo}<button class="small" onclick={gotoRepo}>{branchExists ? t("在图中查看分支") : t("打开仓库")}</button>{/if}
      </div>

      {#if c.repo}
        <label for="t-branch">{t("关联分支")}</label>
        <div class="row">
          <input id="t-branch" bind:value={c.branch} list="t-branches" placeholder={t("选择或输入分支名（可选）")} spellcheck="false" />
          <datalist id="t-branches">
            {#each localBranches as b (b)}<option value={b}></option>{/each}
          </datalist>
          {#if !c.branch && repoTab}
            <button class="small" disabled={creating} onclick={createBranch} title={flowReady ? t("在这个仓库里用 Git Flow 开始一个 feature") : t("在这个仓库里从当前 HEAD 创建并切换到新分支")}>
              {flowReady ? t("开始 Feature") : t("创建分支")}
            </button>
          {/if}
        </div>
      {/if}
    </div>
    {#if c.repo && !repoTab}
      <div class="hint">{t("这个仓库没有打开，打开后可以选择分支")}</div>
    {:else if c.repo && !c.branch && c.title.trim()}
      <div class="hint">{t("点“{0}”会创建 {1}{2}，不点则不做任何 git 操作", flowReady ? t("开始 Feature") : t("创建分支"), flowReady ? repoTab?.flow?.feature : "", branchSlug(c))}</div>
    {/if}

    <div class="buttons">
      {#if !isNew}<button class="danger" onclick={remove}>{t("删除")}</button>{/if}
      {#if !isNew && !c.archived}<button onclick={() => onArchive(c.id)} title={t("从看板上收起，可以在「归档」里恢复")}>{t("归档")}</button>{/if}
      {#if c.archived}<button onclick={restore}>{t("恢复到看板")}</button>{/if}
      <span class="grow"></span>
      <button onclick={onClose}>{t("取消")}</button>
      <button class="primary" onclick={save}>{isNew ? t("创建") : t("保存")}</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 8vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 540px;
    max-width: calc(100vw - 32px);
    max-height: 84vh;
    overflow: auto;
    padding: 18px 20px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
  }
  .title-input {
    height: 36px;
    font-size: 15px;
    font-weight: 600;
  }
  textarea {
    resize: vertical;
  }
  .grid {
    display: grid;
    grid-template-columns: 76px 1fr;
    align-items: center;
    gap: 10px 12px;
    font-size: 13px;
  }
  .grid label,
  .label {
    color: var(--text-dim);
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
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .row input {
    flex: 1;
    min-width: 0;
  }
  .prios {
    display: flex;
    gap: 6px;
  }
  .prio {
    min-width: 46px;
    border-left: 4px solid var(--p);
  }
  .prio.on {
    background: var(--accent);
    border-color: var(--accent);
    border-left-color: var(--p);
    color: #fff;
  }
  .small {
    height: 28px;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .hint {
    color: var(--text-faint);
    font-size: 12px;
  }
  .row select {
    flex: 1;
    min-width: 0;
  }
  .archived {
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--bg-hover);
    color: var(--text-dim);
    font-size: 12.5px;
  }
  .checklist {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ck-head {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-dim);
    font-size: 13px;
  }
  .ck-count {
    font-size: 12px;
  }
  .ck-bar {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--bg-hover);
    overflow: hidden;
  }
  .ck-bar span {
    display: block;
    height: 100%;
    background: var(--add);
  }
  .ck-item {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ck-text {
    flex: 1;
    min-width: 0;
    height: 28px;
    border-color: transparent;
    background: none;
  }
  .ck-text:hover,
  .ck-text:focus {
    border-color: var(--border);
  }
  .ck-text.done {
    color: var(--text-faint);
    text-decoration: line-through;
  }
  .ck-del {
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-faint);
    opacity: 0;
  }
  .ck-item:hover .ck-del {
    opacity: 1;
  }
  .ck-new {
    height: 28px;
  }
  .buttons {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }
  .grow {
    flex: 1;
  }
</style>
