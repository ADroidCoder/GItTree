<script lang="ts">
  import { savedLang, setLang, t, type Lang } from "../lib/i18n";
  import { checkUpdate, updater } from "../lib/updater.svelte";
  import { notifyDialog } from "../lib/notify.svelte";
  import FocusPicker from "./FocusPicker.svelte";
  import { api } from "../lib/api";
  import { app, chooseEditor, dialog, errorText, forcePush, openFlow, openIn, setAutoFetch, setTheme, toast, type RepoTab, type Theme } from "../lib/store.svelte";
  import { isMac } from "../lib/util";
  import RepoPicker from "./RepoPicker.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import FlowPanel from "./FlowPanel.svelte";

  let { tab, onOpenFolder }: { tab: RepoTab | undefined; onOpenFolder: () => void } = $props();

  let picker = $state<{ x: number; y: number } | null>(null);
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

  const THEMES: { value: Theme; label: string; icon: string }[] = [
    { value: "dark", label: t("深色"), icon: "☾" },
    { value: "light", label: t("浅色"), icon: "☀" },
    { value: "system", label: t("跟随系统"), icon: "◐" },
  ];
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
        { label: t("设置编辑器命令…"), action: chooseEditor },
      ],
    };
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

  const AUTO_FETCH = [0, 5, 10, 30];
  function showThemeMenu(e: MouseEvent) {
    flowMenu = {
      ...below(e),
      items: [
        ...THEMES.map((t) => ({
          label: `${t.icon}  ${t.label}${app.theme === t.value ? "  ✓" : ""}`,
          action: () => setTheme(t.value),
        })),
        "sep",
        ...AUTO_FETCH.map((m) => ({
          label: `${m ? t("每 {0} 分钟自动获取", m) : t("不自动获取")}${app.autoFetch === m ? "  ✓" : ""}`,
          action: () => setAutoFetch(m),
        })),
        "sep",
        ...LANGS.map((l) => ({
          label: `${l.label}${savedLang() === l.value ? "  ✓" : ""}`,
          action: () => changeLang(l.value),
        })),
        "sep",
        { label: t("提醒与通知…"), action: () => (notifyDialog.open = true) },
        {
          label: updater.update ? t("安装新版本 {0}…", updater.update.version) : t("检查更新…"),
          hint: updater.version ? `v${updater.version}` : undefined,
          action: () => (updater.update ? (updater.open = true) : checkUpdate(true)),
        },
      ],
    };
  }

  /** 语言名用它自己的语言写，看不懂当前界面也能找到 */
  const LANGS: { value: Lang | null; label: string }[] = [
    { value: null, label: t("🌐  跟随系统语言") },
    { value: "zh", label: "🌐  中文" },
    { value: "en", label: "🌐  English" },
  ];

  function changeLang(l: Lang | null) {
    if (savedLang() === l) return;
    // 切换语言会重新加载界面，正在执行的操作的进度就看不到了
    if (app.tabs.some((x) => x.busy)) return toast(t("有操作正在进行，完成后再切换语言"), "error");
    setLang(l);
  }

  /** 当前分支是 Git Flow 分支时，按钮上显示类型 */
  const curFlow = $derived(tab?.flowBranch(tab?.repo?.head.branch));
  let searchInput = $state<HTMLInputElement>();
  let searching = $state(false);
  let lastQuery = "";

  const head = $derived(tab?.repo?.head);
  const headRef = $derived(tab?.repo?.refs.find((r) => r.is_head));
  const locals = $derived(
    (tab?.repo?.refs ?? [])
      .filter((r) => r.kind === "local" && r.name.toLowerCase().includes(branchFilter.trim().toLowerCase()))
      .sort((a, b) => Number(b.is_head) - Number(a.is_head) || a.name.localeCompare(b.name)),
  );

  export function focusSearch() {
    searchInput?.focus();
    searchInput?.select();
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

  async function doSearch() {
    if (!tab) return;
    const q = tab.search.query.trim();
    lastQuery = q;
    if (!q) {
      tab.search.hits = [];
      return;
    }
    searching = true;
    try {
      const hits = await api.search(tab.id, q);
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
      if (tab.search.query.trim() === lastQuery && tab.search.hits.length) gotoHit(tab.search.index + (e.shiftKey ? -1 : 1));
      else doSearch();
    }
    if (e.key === "Escape") {
      tab.search.query = "";
      tab.search.hits = [];
      lastQuery = "";
      searchInput?.blur();
    }
  }
</script>

<div class="toolbar">
  <div class="left">
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
        <span class="ico">↶</span>{t("撤销")}
      </button>
      <span class="sep"></span>
      <button
        class="tb"
        onclick={() => tab.run({ op: "fetch" }, t("正在获取远程更新…"))}
        title={t("获取所有远程仓库的更新（不修改本地分支）{0}", app.autoFetch ? t("\n每 {0} 分钟自动获取一次，可在右上角设置里修改", app.autoFetch) : "")}
      >
        <span class="ico">⟳</span>{t("获取")}
      </button>
      <button class="tb" onclick={() => tab.run({ op: "pull" }, t("正在拉取…"))} title={t("拉取当前分支")}>
        <span class="ico">⬇</span>{t("拉取")}
        {#if headRef?.behind}<span class="badge">{headRef.behind}</span>{/if}
      </button>
      <button class="tb" onclick={() => tab.run({ op: "push", force: false }, t("正在推送…"))} title={t("推送当前分支")}>
        <span class="ico">⬆</span>{t("推送")}
        {#if headRef?.ahead}<span class="badge">{headRef.ahead}</span>{/if}
      </button>
      <button class="tb more" onclick={showPushMenu} title={t("更多推送选项")}>▾</button>
      <span class="sep"></span>
      <button class="tb" onclick={newBranch} title={t("新建分支")}><span class="ico">⑂</span>{t("分支")}</button>
      <button class="tb" onclick={stash} title={t("Stash 当前改动")}><span class="ico">⊟</span>Stash</button>
      <button
        class="tb"
        disabled={!tab.stashes.length}
        onclick={() => tab.run({ op: "stash", action: "pop", index: 0 }, t("正在恢复 stash…"))}
        title={t("恢复最近一次 stash")}><span class="ico">⊞</span>Pop</button
      >
      <span class="sep"></span>
      <button class="tb" onclick={showFlowMenu} title="Git Flow：feature / release / hotfix">
        <span class="ico">⇶</span>Git Flow
        {#if curFlow}<span class="badge flow">{curFlow.kind}</span>{/if}
      </button>
    </div>

    <div class="right">
      <button class="tb" onclick={showOpenMenu} title={t("用编辑器、终端或文件管理器打开这个仓库")}>
        <span class="ico">↗</span>{t("打开")}
      </button>
      <button class="tb" class:on={tab.logOpen} onclick={() => (tab.logOpen = !tab.logOpen)} title={t("查看 git 命令和输出日志")}>
        <span class="ico">≡</span>{t("日志")}
      </button>
      <div class="search">
        <span class="sico">⌕</span>
        <input
          bind:this={searchInput}
          bind:value={tab.search.query}
          placeholder={t("搜索提交说明、作者、哈希 ({0}F)", isMac ? "⌘" : "Ctrl+")}
          onkeydown={searchKey}
        />
        {#if searching}
          <span class="hits">{t("搜索中…")}</span>
        {:else if tab.search.hits.length}
          <span class="hits">{tab.search.index + 1}/{tab.search.hits.length}{tab.search.hits.length >= 5000 ? "+" : ""}</span>
          <button class="nav" onclick={() => gotoHit(tab.search.index - 1)}>▲</button>
          <button class="nav" onclick={() => gotoHit(tab.search.index + 1)}>▼</button>
        {/if}
      </div>
      <button class="tb icon" onclick={() => tab.refresh()} title={t("刷新")}>↻</button>
    </div>
  {/if}
  <button class="tb icon theme" onclick={showThemeMenu} title={t("设置：主题、自动获取、语言")}>⚙</button>
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
    gap: 12px;
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
  .right {
    justify-content: flex-end;
  }
  .center {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .picker-group {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .plabel {
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
    min-width: 54px;
    height: 44px;
    padding: 0 8px;
    border: 0;
    background: none;
    font-size: 11.5px;
    color: var(--text-dim);
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
    margin: 0 6px;
    background: var(--border);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 4px;
    position: relative;
  }
  .sico {
    position: absolute;
    left: 9px;
    color: var(--text-faint);
    pointer-events: none;
  }
  .search input {
    width: 260px;
    padding-left: 26px;
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
