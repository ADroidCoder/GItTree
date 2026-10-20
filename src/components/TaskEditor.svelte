<script lang="ts">
  import { PRIORITIES, branchSlug, type Card, type Column } from "../lib/board";
  import { activate, app, confirm, openRepo, recentRepos } from "../lib/store.svelte";

  let {
    card,
    columns,
    assignees,
    isNew,
    onSave,
    onDelete,
    onClose,
  }: {
    card: Card;
    columns: Column[];
    assignees: string[];
    isNew: boolean;
    onSave: (c: Card) => void;
    onDelete: (id: string) => void;
    onClose: () => void;
  } = $props();

  // 在副本上编辑，点保存才写回
  // svelte-ignore state_referenced_locally
  let c = $state<Card>({ ...card });
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
    onSave({ ...c });
  }

  /** 可选操作：在关联的仓库里为任务创建分支（初始化了 Git Flow 就开始一个 feature） */
  async function createBranch() {
    const t = repoTab;
    if (!t || !c.title.trim()) return titleEl?.focus();
    const slug = branchSlug(c);
    creating = true;
    const name = flowReady ? `${t.flow!.feature}${slug}` : slug;
    const ok = flowReady
      ? await t.run({ op: "flow_start", kind: "feature", name: slug, base: null }, `正在创建 ${name}…`)
      : await t.run({ op: "create_branch", name, start: "HEAD", checkout: true }, `正在创建 ${name}…`);
    creating = false;
    if (ok) c.branch = name;
  }

  async function gotoRepo() {
    save();
    await openRepo(c.repo);
    const t = app.tabs.find((x) => x.id === c.repo);
    const ref = t?.repo?.refs.find((r) => r.kind === "local" && r.name === c.branch);
    if (t && ref?.row != null) {
      activate(t);
      t.select(ref.row, ref.oid);
      t.scrollTo(ref.row);
    }
  }

  async function remove() {
    if (await confirm("删除任务", `确定删除 “${c.title || "（无标题）"}” 吗？`, "删除", true)) onDelete(c.id);
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
      placeholder="任务标题"
      onkeydown={(e) => e.key === "Enter" && !e.isComposing && save()}
    />
    <textarea bind:value={c.description} rows="5" placeholder="描述（可选）"></textarea>

    <div class="grid">
      <label for="t-col">状态</label>
      <select id="t-col" bind:value={c.column}>
        {#each columns as col (col.id)}<option value={col.id}>{col.name}</option>{/each}
      </select>

      <label for="t-who">负责人</label>
      <input id="t-who" bind:value={c.assignee} list="t-assignees" placeholder="姓名" />
      <datalist id="t-assignees">
        {#each assignees as a (a)}<option value={a}></option>{/each}
      </datalist>

      <span class="label">优先级</span>
      <div class="prios">
        {#each PRIORITIES as p (p.value)}
          <button class="prio" class:on={c.priority === p.value} style="--p:{p.color}" onclick={() => (c.priority = p.value)}>
            {p.label}
          </button>
        {/each}
      </div>

      <label for="t-due">截止日期</label>
      <div class="row">
        <input id="t-due" type="date" bind:value={c.due} />
        {#if c.due}<button class="small" onclick={() => (c.due = "")}>清除</button>{/if}
      </div>

      <label for="t-repo">关联仓库</label>
      <div class="row">
        <select id="t-repo" bind:value={c.repo}>
          <option value="">不关联</option>
          {#each repoOptions as r (r)}<option value={r}>{repoName(r)} — {r}</option>{/each}
        </select>
        {#if c.repo}<button class="small" onclick={gotoRepo}>{branchExists ? "在图中查看分支" : "打开仓库"}</button>{/if}
      </div>

      {#if c.repo}
        <label for="t-branch">关联分支</label>
        <div class="row">
          <input id="t-branch" bind:value={c.branch} list="t-branches" placeholder="选择或输入分支名（可选）" spellcheck="false" />
          <datalist id="t-branches">
            {#each localBranches as b (b)}<option value={b}></option>{/each}
          </datalist>
          {#if !c.branch && repoTab}
            <button class="small" disabled={creating} onclick={createBranch} title={flowReady ? "在这个仓库里用 Git Flow 开始一个 feature" : "在这个仓库里从当前 HEAD 创建并切换到新分支"}>
              {flowReady ? "开始 Feature" : "创建分支"}
            </button>
          {/if}
        </div>
      {/if}
    </div>
    {#if c.repo && !repoTab}
      <div class="hint">这个仓库没有打开，打开后可以选择分支</div>
    {:else if c.repo && !c.branch && c.title.trim()}
      <div class="hint">点“{flowReady ? "开始 Feature" : "创建分支"}”会创建 {flowReady ? repoTab?.flow?.feature : ""}{branchSlug(c)}，不点则不做任何 git 操作</div>
    {/if}

    <div class="buttons">
      {#if !isNew}<button class="danger" onclick={remove}>删除</button>{/if}
      <span class="grow"></span>
      <button onclick={onClose}>取消</button>
      <button class="primary" onclick={save}>{isNew ? "创建" : "保存"}</button>
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
  .buttons {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }
  .grow {
    flex: 1;
  }
</style>
