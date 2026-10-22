<script lang="ts">
  import { tick } from "svelte";
  import { t } from "../lib/i18n";
  import { checkUpdate, updater } from "../lib/updater.svelte";
  import { shareUrl } from "../lib/util";
  import FocusPicker from "./FocusPicker.svelte";
  import { api, type SearchKind } from "../lib/api";
  import { addRemote, addSubmodule, addWorktree, app, copyRemoteUrl, createPull, dialog, errorText, forcePush, openRemoteWeb, openFlow, openIn, openSettings, startBisect, toast, trackWithLfs, type RepoTab } from "../lib/store.svelte";
  import { isMac } from "../lib/util";
  import RepoPicker from "./RepoPicker.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import FlowPanel from "./FlowPanel.svelte";

  let { tab, onOpenFolder }: { tab: RepoTab | undefined; onOpenFolder: () => void } = $props();

  let picker = $state<{ x: number; y: number } | null>(null);

  /**
   * 窗口窄到放不下时逐级收起（分支名会先自己省略一部分）：
   * 1 级右边的「打开、日志」只留图标、搜索框变窄；2 级中间的按钮也只留图标；3 级搜索框收成一个按钮（点开浮在下面）。
   * 每次尺寸变化都先试着展开一级、放不下再收回去；改完等 DOM 更新后立刻量，都在绘制前完成，不会闪。
   */
  let bar = $state<HTMLDivElement>();
  let leftEl = $state<HTMLDivElement>();
  let rightEl = $state<HTMLDivElement>();
  let level = $state(0);
  const MAX_LEVEL = 3;
  const tight = $derived(level >= MAX_LEVEL);
  /** 收成按钮后点开的搜索框 */
  let searchOpen = $state(false);
  let fitting = false;
  let fitAgain = false;
  function overflows() {
    const over = (el?: HTMLElement) => (el ? Math.max(0, el.scrollWidth - el.clientWidth) : 0);
    return over(leftEl) + over(rightEl) + over(bar) > 1;
  }
  async function fit() {
    if (!bar) return;
    // 正在调整时又变了尺寸：这轮结束后再来一次
    if (fitting) {
      fitAgain = true;
      return;
    }
    fitting = true;
    try {
      while (level > 0) {
        level--;
        await tick();
        if (overflows()) {
          level++;
          await tick();
          break;
        }
      }
      while (level < MAX_LEVEL && overflows()) {
        level++;
        await tick();
      }
    } finally {
      fitting = false;
    }
    if (fitAgain) {
      fitAgain = false;
      fit();
    }
  }
  $effect(() => {
    if (!bar) return;
    const ro = new ResizeObserver(fit);
    ro.observe(bar);
    return () => ro.disconnect();
  });
  // 分支名、语言变了也可能放不下
  $effect(() => {
    void [tab?.name, tab?.repo?.head?.branch, focusLabel, tab?.search.hits.length, tab?.search.kind, tab?.repo];
    tick().then(fit);
  });
  let branchMenu = $state<{ x: number; y: number } | null>(null);
  let focusPicker = $state<{ x: number; y: number } | null>(null);

  /** 分支筛选按钮上的文字 */
  const focusLabel = $derived.by(() => {
    const f = tab?.focus ?? [];
    if (!f.length) return t("全部分支");
    if (f.length === 1 && f[0] === "HEAD") return t("当前分支");
    if (f.length === 1) return f[0].replace(/^refs\/(heads|remotes)\//, "");
    return t("{0} 个分支", f.length);
  });
  let branchFilter = $state("");
  let flowMenu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  /** Git Flow 面板：开始新分支，或完成 / 推送已有的 flow 分支 */
  let flowPanel = $state<{ x: number; y: number } | null>(null);

  function showFlowMenu(e: MouseEvent) {
    if (!tab) return;
    const tb = tab;
    const pos = below(e);
    if (!tb.flow?.initialized) {
      flowMenu = { ...pos, items: [{ label: t("初始化 Git Flow…"), action: () => openFlow({ tab: tb, mode: "init" }) }] };
      return;
    }
    flowPanel = pos;
  }

  function showOpenMenu(e: MouseEvent) {
    if (!tab) return;
    const tb = tab;
    flowMenu = {
      ...below(e),
      items: [
        { label: t("用编辑器打开"), hint: app.editor, action: () => openIn(tb, "editor") },
        { label: t("在终端中打开"), action: () => openIn(tb, "terminal") },
        { label: isMac ? t("在访达中显示") : t("在资源管理器中显示"), action: () => openIn(tb, "folder") },
        "sep",
        // 有多个远程仓库时每个都列出来
        ...tb.remotes.flatMap((r): MenuItem[] => [
          { label: tb.remotes.length > 1 ? t("复制 {0} 的地址", r.name) : t("复制仓库地址"), hint: shareUrl(r.url), action: () => copyRemoteUrl(r) },
          { label: tb.remotes.length > 1 ? t("在浏览器中打开 {0}", r.name) : t("在浏览器中打开仓库"), action: () => openRemoteWeb(r) },
        ]),
        ...(tb.remotes.length ? [] : [{ label: t("添加远程仓库…"), action: () => addRemote(tb) }]),
        "sep",
        { label: t("设置编辑器命令…"), action: () => openSettings("tools") },
      ],
    };
  }

  function showToolsMenu(e: MouseEvent) {
    if (!tab) return;
    const tb = tab;
    const lfs = tb.extras?.lfs;
    const email = tb.extras?.identity.email;
    const items: MenuItem[] = [
      { label: t("找回丢失的提交（Reflog）…"), action: () => tb.openTool("reflog") },
      "sep",
      { label: t("Pull Request / Merge Request"), disabled: !tb.remotes.length, action: () => tb.openTool("pulls") },
      { label: t("为当前分支创建 PR…"), disabled: !tb.remotes.length || !tb.repo?.head.branch, action: () => createPull(tb) },
      "sep",
      { label: t("新建工作树…"), hint: "worktree", action: () => addWorktree(tb) },
      ...(tb.extras?.submodules.length
        ? [{ label: t("更新全部子模块"), action: () => tb.run({ op: "submodule_update", path: null }, t("正在更新子模块…")) }]
        : []),
      { label: t("添加子模块…"), action: () => addSubmodule(tb) },
      "sep",
      tb.bisect?.active
        ? { label: t("结束二分查错"), action: () => tb.run({ op: "bisect_reset" }, t("正在结束二分查错…")) }
        : { label: t("开始二分查错…"), hint: "bisect", action: () => startBisect(tb) },
      "sep",
    ];
    if (lfs?.used && !lfs.installed) {
      items.push({ label: t("Git LFS：没有安装 git-lfs"), disabled: true, action: () => {} });
    } else if (lfs?.installed) {
      items.push(
        { label: t("Git LFS：拉取大文件"), disabled: !lfs.used, action: () => tb.run({ op: "lfs_pull" }, t("正在拉取 LFS 文件…")) },
        { label: t("Git LFS：管理一类文件…"), hint: lfs.patterns.slice(0, 3).join(" "), action: () => trackWithLfs(tb, null) },
      );
    }
    items.push("sep", { label: t("提交身份…"), hint: email ?? t("未设置"), action: () => (app.identityDialog = tb) });
    flowMenu = { ...below(e), items };
  }

  function showPushMenu(e: MouseEvent) {
    if (!tab) return;
    const tb = tab;
    flowMenu = {
      ...below(e),
      items: [
        { label: t("推送"), action: () => tb.run({ op: "push", force: false }, t("正在推送…")) },
        { label: t("强制推送…"), hint: "--force-with-lease", danger: true, action: () => forcePush(tb) },
      ],
    };
  }

  /** 当前分支是 Git Flow 分支时，按钮上显示类型 */
  const curFlow = $derived(tab?.flowBranch(tab?.repo?.head.branch));
  let searchInput = $state<HTMLInputElement>();
  let fromInput = $state<HTMLInputElement>();
  let searching = $state(false);
  let lastQuery = "";

  const head = $derived(tab?.repo?.head);
  const headRef = $derived(tab?.repo?.refs.find((r) => r.is_head));
  const locals = $derived(
    (tab?.repo?.refs ?? [])
      .filter((r) => r.kind === "local" && r.name.toLowerCase().includes(branchFilter.trim().toLowerCase()))
      .sort((a, b) => Number(b.is_head) - Number(a.is_head) || a.name.localeCompare(b.name)),
  );

  /** 不指定方式时：正在按日期搜就换回按说明搜 */
  export async function focusSearch(kind?: SearchKind) {
    if (!tab) return;
    searchOpen = true;
    setKind(kind ?? (tab.search.kind === "date" ? "text" : tab.search.kind));
    await tick();
    if (tab.search.kind === "date") fromInput?.focus();
    else {
      searchInput?.focus();
      searchInput?.select();
    }
  }

  function below(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return { x: r.left, y: r.bottom + 4 };
  }

  export async function newBranch() {
    if (!tab) return;
    const start = tab.selectedOid ?? "HEAD";
    const r = await dialog({
      title: t("新建分支"),
      message: tab.selectedOid ? t("从选中的提交 {0} 创建", tab.selectedOid.slice(0, 7)) : t("从当前 HEAD 创建"),
      input: { placeholder: t("分支名称"), value: "" },
      checkbox: { label: t("创建后切换到新分支"), value: true },
      confirmText: t("创建"),
    });
    if (r?.input.trim()) await tab.run({ op: "create_branch", name: r.input.trim(), start, checkout: r.checkbox }, t("正在创建分支…"));
  }

  export async function stash() {
    if (!tab) return;
    const r = await dialog({
      title: t("暂存工作区 (Stash)"),
      message: t("包括未跟踪的文件"),
      input: { placeholder: t("说明（可选）"), value: "" },
      confirmText: "Stash",
    });
    if (r) await tab.run({ op: "stash_push", message: r.input || null }, t("正在 stash…"));
  }

  const KINDS: { kind: SearchKind; short: string; label: string }[] = [
    { kind: "text", short: t("说明"), label: t("提交说明、作者、哈希") },
    { kind: "path", short: t("路径"), label: t("改过的文件路径") },
    { kind: "code", short: t("代码"), label: t("改动的代码内容") },
    { kind: "date", short: t("日期"), label: t("提交日期范围") },
  ];
  const kindInfo = $derived(KINDS.find((k) => k.kind === tab?.search.kind) ?? KINDS[0]);
  const placeholder = $derived.by(() => {
    const key = isMac ? "⌘F" : "Ctrl+F";
    if (kindInfo.kind === "path") return t("搜索改过的文件，如 {0}", "src/app");
    if (kindInfo.kind === "code") return t("搜索改动里的代码 ({0})", key);
    return t("搜索提交说明、作者、哈希 ({0}F)", isMac ? "⌘" : "Ctrl+");
  });

  function showKindMenu(e: MouseEvent) {
    if (!tab) return;
    const tb = tab;
    flowMenu = {
      ...below(e),
      items: KINDS.map((k) => ({
        label: (k.kind === tb.search.kind ? "✓ " : "　 ") + k.label,
        action: () => focusSearch(k.kind),
      })),
    };
  }

  function setKind(kind: SearchKind) {
    if (!tab || tab.search.kind === kind) return;
    tab.search.kind = kind;
    tab.search.hits = [];
    lastQuery = "";
    if (kind === "date") {
      if (tab.search.from || tab.search.to) doSearch();
      return;
    }
    // 换了方式后用输入框里已有的文字重新搜
    if (tab.search.query.trim()) doSearch();
  }

  /** 本地日期 → 秒；end 为真时取这一天的最后一秒 */
  function daySec(d: string, end: boolean) {
    if (!d) return "";
    const [y, m, day] = d.split("-").map(Number);
    return String(Math.floor(new Date(y, m - 1, day + (end ? 1 : 0)).getTime() / 1000) - (end ? 1 : 0));
  }

  /** 发给后台的搜索内容 */
  function queryOf(tb: RepoTab) {
    const s = tb.search;
    if (s.kind !== "date") return s.query.trim();
    return s.from || s.to ? `${daySec(s.from, false)}..${daySec(s.to, true)}` : "";
  }

  async function doSearch() {
    if (!tab) return;
    const kind = tab.search.kind;
    const q = queryOf(tab);
    lastQuery = q;
    if (!q) {
      tab.search.hits = [];
      return;
    }
    searching = true;
    try {
      const hits = await api.search(tab.id, q, kind);
      tab.search.hits = hits;
      tab.search.index = 0;
      if (hits.length === 0) toast(t("没有找到匹配的提交"));
      else gotoHit(0);
    } catch (err) {
      toast(errorText(err), "error");
    } finally {
      searching = false;
    }
  }

  async function gotoHit(i: number) {
    if (!tab) return;
    const hits = tab.search.hits;
    if (!hits.length) return;
    const idx = (i + hits.length) % hits.length;
    tab.search.index = idx;
    const row = hits[idx];
    const res = await api.getRows(tab.id, row, row + 1);
    if (res.rows[0]) tab.select(row, res.rows[0].oid);
    tab.scrollTo(row);
  }

  function searchKey(e: KeyboardEvent) {
    if (!tab || e.isComposing) return;
    if (e.key === "Enter") {
      e.preventDefault();
      // 已经有结果时回车跳到下一个，Shift+回车跳到上一个
      if (queryOf(tab) === lastQuery && tab.search.hits.length) gotoHit(tab.search.index + (e.shiftKey ? -1 : 1));
      else doSearch();
    }
    if (e.key === "Escape") {
      tab.search.query = "";
      tab.search.from = tab.search.to = "";
      searchOpen = false;
      tab.search.hits = [];
      lastQuery = "";
      searchInput?.blur();
    }
  }
</script>

<div class="toolbar" class:lv1={level >= 1} class:lv2={level >= 2} bind:this={bar}>
  <div class="left" bind:this={leftEl}>
    <div class="picker-group">
      <span class="plabel">{t("仓库")}</span>
      <button class="picker" onclick={(e) => (picker = below(e))}>
        <span class="pval">{tab?.name ?? t("打开仓库")}</span>
        <span class="caret">▾</span>
      </button>
    </div>
    {#if tab?.repo}
      <span class="slash">/</span>
      <div class="picker-group">
        <span class="plabel">{t("分支")}</span>
        <button
          class="picker"
          onclick={(e) => {
            branchFilter = "";
            branchMenu = below(e);
          }}
        >
          <span class="pval">
            {#if head?.branch}{head.branch}{:else if head?.oid}{t("分离 {0}", head.oid.slice(0, 7))}{:else}—{/if}
          </span>
          {#if headRef?.ahead}<span class="up">{headRef.ahead}↑</span>{/if}
          {#if headRef?.behind}<span class="down">{headRef.behind}↓</span>{/if}
          <span class="caret">▾</span>
        </button>
      </div>
      <button
        class="focus-btn"
        class:on={tab.focus.length > 0}
        onclick={(e) => (focusPicker = below(e))}
        title={t("选择在提交图里显示哪些分支")}
      >
        ◎ {focusLabel}
        <span class="caret">▾</span>
      </button>
    {/if}
  </div>

  {#if tab?.repo}
    <div class="center">
      <button
        class="tb"
        disabled={!tab.repo.undo || !!tab.busy}
        onclick={() => tab.undo()}
        title={tab.repo.undo ? t("撤销：{0} ({1}Z)", tab.repo.undo, isMac ? "⌘" : "Ctrl+") : t("没有可以撤销的操作")}
      >
        <span class="ico">↶</span><span class="lbl">{t("撤销")}</span>
      </button>
      <span class="sep"></span>
      <button
        class="tb"
        onclick={() => tab.run({ op: "fetch" }, t("正在获取远程更新…"))}
        title={t("获取所有远程仓库的更新（不修改本地分支）{0}", app.autoFetch ? t("\n每 {0} 分钟自动获取一次，可在右上角设置里修改", app.autoFetch) : "")}
      >
        <span class="ico">⟳</span><span class="lbl">{t("获取")}</span>
      </button>
      <button class="tb" onclick={() => tab.run({ op: "pull" }, t("正在拉取…"))} title={t("拉取当前分支")}>
        <span class="ico">⬇</span><span class="lbl">{t("拉取")}</span>
        {#if headRef?.behind}<span class="badge">{headRef.behind}</span>{/if}
      </button>
      <button class="tb" onclick={() => tab.run({ op: "push", force: false }, t("正在推送…"))} title={t("推送当前分支")}>
        <span class="ico">⬆</span><span class="lbl">{t("推送")}</span>
        {#if headRef?.ahead}<span class="badge">{headRef.ahead}</span>{/if}
      </button>
      <button class="tb more" onclick={showPushMenu} title={t("更多推送选项")}>▾</button>
      <span class="sep"></span>
      <button class="tb" onclick={newBranch} title={t("新建分支")}><span class="ico">⑂</span><span class="lbl">{t("分支")}</span></button>
      <button class="tb" onclick={stash} title={t("Stash 当前改动")}><span class="ico">⊟</span><span class="lbl">Stash</span></button>
      <button
        class="tb"
        disabled={!tab.stashes.length}
        onclick={() => tab.run({ op: "stash", action: "pop", index: 0 }, t("正在恢复 stash…"))}
        title={t("恢复最近一次 stash")}><span class="ico">⊞</span><span class="lbl">Pop</span></button
      >
      <span class="sep"></span>
      <button class="tb" onclick={showFlowMenu} title="Git Flow：feature / release / hotfix">
        <span class="ico">⇶</span><span class="lbl">Git Flow</span>
        {#if curFlow}<span class="badge flow">{curFlow.kind}</span>{/if}
      </button>
      <button class="tb" onclick={showToolsMenu} title={t("Reflog、PR、工作树、子模块、二分查错、LFS、提交身份")}>
        <span class="ico">⋯</span><span class="lbl">{t("更多")}</span>
        {#if tab.bisect?.active}<span class="badge flow">bisect</span>{/if}
      </button>
    </div>

    <div class="right" bind:this={rightEl}>
      <button class="tb" onclick={showOpenMenu} title={t("用编辑器、终端或文件管理器打开这个仓库")}>
        <span class="ico">↗</span><span class="lbl">{t("打开")}</span>
      </button>
      <button class="tb" class:on={tab.logOpen} onclick={() => (tab.logOpen = !tab.logOpen)} title={t("查看 git 命令和输出日志")}>
        <span class="ico">≡</span><span class="lbl">{t("日志")}</span>
      </button>
      {#if tight}
        <button class="tb icon" class:on={searchOpen || tab.search.hits.length > 0} onclick={() => (searchOpen ? (searchOpen = false) : focusSearch())} title={t("搜索提交 ({0}F)", isMac ? "⌘" : "Ctrl+")}>⌕</button>
      {/if}
      {#if !tight || searchOpen}
        <div class="search" class:wide={tab.search.kind === "date"} class:pop={tight}>
          <button class="kind" onclick={showKindMenu} title={t("搜索方式：{0}", kindInfo.label)}>⌕ {kindInfo.short}<span class="caret">▾</span></button>
          {#if tab.search.kind === "date"}
            <input class="day" type="date" bind:this={fromInput} bind:value={tab.search.from} max={tab.search.to || undefined} onchange={doSearch} onkeydown={searchKey} title={t("从这天起")} />
            <span class="dash">–</span>
            <input class="day" type="date" bind:value={tab.search.to} min={tab.search.from || undefined} onchange={doSearch} onkeydown={searchKey} title={t("到这天为止")} />
          {:else}
            <input bind:this={searchInput} bind:value={tab.search.query} {placeholder} onkeydown={searchKey} />
          {/if}
          {#if searching}
            <span class="hits">{t("搜索中…")}</span>
          {:else if tab.search.hits.length}
            <span class="hits">{tab.search.index + 1}/{tab.search.hits.length}{tab.search.hits.length >= 5000 ? "+" : ""}</span>
            <button class="nav" onclick={() => gotoHit(tab.search.index - 1)}>▲</button>
            <button class="nav" onclick={() => gotoHit(tab.search.index + 1)}>▼</button>
          {/if}
        </div>
      {/if}
      <button class="tb icon" onclick={() => tab.refresh()} title={t("刷新")}>↻</button>
    </div>
  {/if}
  <button class="tb icon theme" onclick={() => openSettings()} title={t("设置 ({0},)", isMac ? "⌘" : "Ctrl+")}>⚙</button>
</div>

{#if picker}
  <RepoPicker x={picker.x} y={picker.y} {onOpenFolder} onClose={() => (picker = null)} />
{/if}

{#if flowPanel && tab}
  <FlowPanel {tab} x={flowPanel.x} y={flowPanel.y} onClose={() => (flowPanel = null)} />
{/if}
{#if flowMenu}
  <ContextMenu x={flowMenu.x} y={flowMenu.y} items={flowMenu.items} onClose={() => (flowMenu = null)} />
{/if}

{#if focusPicker && tab}
  <FocusPicker {tab} x={focusPicker.x} y={focusPicker.y} onClose={() => (focusPicker = null)} />
{/if}

{#if branchMenu && tab}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={() => (branchMenu = null)}></div>
  <div class="bmenu" style="left:{branchMenu.x}px; top:{branchMenu.y}px">
    <!-- svelte-ignore a11y_autofocus -->
    <input
      placeholder={t("筛选或切换本地分支")}
      bind:value={branchFilter}
      autofocus
      onkeydown={(e) => {
        if (e.key === "Escape") branchMenu = null;
        if (e.key === "Enter" && locals[0] && !locals[0].is_head) {
          branchMenu = null;
          tab.run({ op: "checkout", name: locals[0].name }, t("正在切换到 {0}…", locals[0].name));
        }
      }}
    />
    <div class="blist">
      {#each locals as r (r.full)}
        <button
          class="bitem"
          class:head={r.is_head}
          onclick={() => {
            branchMenu = null;
            if (!r.is_head) tab.run({ op: "checkout", name: r.name }, t("正在切换到 {0}…", r.name));
          }}
        >
          <span class="mark">{r.is_head ? "✓" : ""}</span>
          <span class="bname">{r.name}</span>
          {#if r.ahead}<span class="up">{r.ahead}↑</span>{/if}
          {#if r.behind}<span class="down">{r.behind}↓</span>{/if}
        </button>
      {:else}
        <div class="bempty">{t("没有匹配的分支")}</div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    position: relative;
    height: 52px;
    padding: 0 12px;
    background: var(--bg-toolbar);
    border-bottom: 1px solid var(--border);
    user-select: none;
  }
  .left,
  .right {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
  }
  /* 用 margin-left: auto 靠右而不是 justify-content: flex-end：放不下时才会溢出到右边、量得出来 */
  .right > :first-child {
    margin-left: auto;
  }
  /* 右边按内容占宽、不被压缩：放不下时整个工具栏溢出，由 fit() 量出来再收起；多余的宽度留给左边的仓库名和分支名 */
  .right {
    min-width: max-content;
  }
  /* 左边以内容宽度为起点，窄的时候先把右边多出来的空让给它 */
  .left {
    flex: 1 1 auto;
  }
  /* 仓库名和分支名各留一个最小宽度：压到这里还放不下，左边就算溢出，fit() 会先把按钮收成图标、搜索框收成按钮 */
  .left > .picker-group {
    min-width: 110px;
  }
  .left > .picker-group:first-child {
    min-width: 80px;
  }
  .center {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 2px;
  }
  .picker-group {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .left > .picker-group:first-child {
    flex-shrink: 2;
  }
  .plabel {
    white-space: nowrap;
    padding-left: 8px;
    color: var(--text-faint);
    font-size: 10.5px;
  }
  .picker {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    max-width: 240px;
    min-width: 0;
    padding: 0 8px;
    border: 0;
    background: none;
    font-weight: 600;
  }
  .picker:hover {
    background: var(--bg-hover);
  }
  .pval {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .focus-btn {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    height: 24px;
    margin-top: 12px;
    padding: 0 10px;
    border-radius: 12px;
    background: none;
    color: var(--text-dim);
    font-size: 12px;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .lv1 .focus-btn {
    flex-shrink: 1;
    min-width: 60px;
  }
  .focus-btn.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .focus-btn.on .caret {
    color: #fff;
  }
  .slash {
    color: var(--text-faint);
    margin-top: 12px;
  }
  .caret {
    font-size: 9px;
    color: var(--text-dim);
  }
  .up {
    color: var(--add);
    font-size: 11.5px;
    font-weight: normal;
  }
  .down {
    color: var(--warn);
    font-size: 11.5px;
    font-weight: normal;
  }
  .tb {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    min-width: 36px;
    height: 44px;
    padding: 0 5px;
    border: 0;
    background: none;
    font-size: 11.5px;
    color: var(--text-dim);
    white-space: nowrap;
  }
  /* 推送旁边的下拉箭头，紧贴推送按钮 */
  .tb.more {
    min-width: 0;
    width: 16px;
    margin-left: -4px;
    padding: 0;
    font-size: 10px;
  }
  .tb:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text);
  }
  .tb.on {
    background: var(--bg-selected);
    color: var(--text);
  }
  /* 收起时按钮只留图标，文字看 title 提示：1 级先收右边，2 级再收中间 */
  .lv1 .right .tb,
  .lv2 .center .tb {
    min-width: 34px;
    padding: 0 6px;
  }
  .lv1 .right .tb .lbl,
  .lv2 .center .tb .lbl {
    display: none;
  }
  .lv1 .right .tb .ico,
  .lv2 .center .tb .ico {
    font-size: 18px;
  }
  .lv2 .center .tb.more {
    min-width: 0;
    padding: 0;
  }
  .lv2 .sep {
    margin: 0 3px;
  }
  .tb.icon {
    min-width: 36px;
    font-size: 18px;
  }
  .tb.theme {
    margin-left: auto;
    flex-shrink: 0;
  }
  .ico {
    font-size: 17px;
    line-height: 1;
  }
  .badge {
    position: absolute;
    top: 3px;
    right: 6px;
    min-width: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-size: 10px;
    line-height: 15px;
  }
  .badge.flow {
    right: auto;
    left: 50%;
    top: auto;
    bottom: -2px;
    transform: translateX(-50%);
    background: var(--add);
    color: #10131a;
  }
  .sep {
    width: 1px;
    height: 26px;
    margin: 0 4px;
    background: var(--border);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 4px;
    position: relative;
    flex: 0 0 200px;
  }
  .lv1 .search {
    flex-basis: 160px;
  }
  .search.wide,
  .lv1 .search.wide {
    flex-basis: 300px;
  }
  /* 窗口太窄时搜索框浮在工具栏下面 */
  .search.pop {
    position: absolute;
    top: calc(100% + 4px);
    right: 52px;
    z-index: 40;
    width: 340px;
    padding: 6px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }
  .kind {
    flex-shrink: 0;
    height: 26px;
    padding: 0 6px;
    color: var(--text-dim);
    font-size: 12px;
    white-space: nowrap;
  }
  .caret {
    margin-left: 2px;
    font-size: 9px;
    color: var(--text-faint);
  }
  .search input {
    flex: 1;
    width: 0;
    min-width: 0;
  }
  .search input.day {
    padding: 0 4px;
    font-size: 12px;
  }
  .dash {
    color: var(--text-faint);
  }
  .hits {
    font-size: 12px;
    color: var(--text-dim);
    white-space: nowrap;
  }
  .nav {
    height: 22px;
    width: 22px;
    padding: 0;
    font-size: 10px;
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
  }
  .bmenu {
    position: fixed;
    z-index: 61;
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 320px;
    padding: 8px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }
  .blist {
    max-height: 50vh;
    overflow: auto;
  }
  .bitem {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 28px;
    border: 0;
    background: none;
    text-align: left;
  }
  .bitem:hover {
    background: var(--bg-hover);
  }
  .bitem.head {
    font-weight: 600;
  }
  .mark {
    width: 14px;
    color: var(--add);
  }
  .bname {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bempty {
    padding: 10px;
    color: var(--text-faint);
    text-align: center;
  }
</style>
