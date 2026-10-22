<script lang="ts">
  import type { FlowKind, SearchKind } from "../lib/api";
  import { bestScore } from "../lib/fuzzy";
  import { lang, setLang, t } from "../lib/i18n";
  import { PRESETS, setTheme, themes } from "../lib/themes.svelte";
  import { activate, activeTab, addRemote, addSubmodule, addWorktree, app, closeTab, copyRemoteUrl, createPull, forcePush, openRemoteWeb, openFlow, openIn, openRepo, openSettings, recentRepos, showTasks, startBisect, trackWithLfs } from "../lib/store.svelte";
  import { checkUpdate } from "../lib/updater.svelte";
  import { shareUrl } from "../lib/util";

  let {
    toolbar,
    onOpenFolder,
    onClose,
  }: {
    toolbar: { newBranch: () => void; stash: () => void; focusSearch: (kind?: SearchKind) => void } | undefined;
    onOpenFolder: () => void;
    onClose: () => void;
  } = $props();

  interface Item {
    group: string;
    label: string;
    /** 右边的灰色提示，比如快捷键 */
    hint?: string;
    /** 额外用来匹配的词：拼音、拼音首字母、英文 */
    keys?: string;
    run: () => void;
  }

  const isMac = navigator.platform.toLowerCase().includes("mac");
  const mod = isMac ? "⌘" : "Ctrl+";
  const tab = activeTab();
  const repo = app.view === "repo" ? tab?.repo : undefined;
  const repoName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

  function commands(): Item[] {
    const g = t("命令");
    const list: Item[] = [];
    if (tab && repo) {
      const tb = tab;
      list.push(
        { group: g, label: t("获取"), keys: "huoqu hq fetch", run: () => tb.run({ op: "fetch" }, t("正在获取远程更新…")) },
        { group: g, label: t("拉取"), keys: "laqu lq pull", run: () => tb.run({ op: "pull" }, t("正在拉取…")) },
        { group: g, label: t("推送"), keys: "tuisong ts push", run: () => tb.run({ op: "push", force: false }, t("正在推送…")) },
        { group: g, label: t("强制推送…"), keys: "qiangzhi tuisong qzts force push", run: () => forcePush(tb) },
        { group: g, label: t("新建分支…"), keys: "xinjian fenzhi xjfz new branch create", run: () => toolbar?.newBranch() },
        { group: g, label: t("Stash 当前改动"), keys: "zancun zc stash save", run: () => toolbar?.stash() },
        { group: g, label: t("恢复最近一次 stash"), keys: "huifu hf stash pop", run: () => tb.run({ op: "stash", action: "pop", index: 0 }, t("正在恢复 stash…")) },
        { group: g, label: t("撤销"), hint: `${mod}Z`, keys: "chexiao cx undo", run: () => tb.undo() },
        { group: g, label: t("刷新"), hint: `${mod}R`, keys: "shuaxin sx refresh reload", run: () => tb.refresh() },
        { group: g, label: t("对比分支…"), keys: "duibi db compare diff branch", run: () => tb.openCompare(repo.head.branch ?? "HEAD", "") },
        { group: g, label: t("搜索提交"), hint: `${mod}F`, keys: "sousuo ss search find commit", run: () => toolbar?.focusSearch() },
        { group: g, label: t("按文件路径搜索提交"), keys: "lujing lj wenjian path file search", run: () => toolbar?.focusSearch("path") },
        { group: g, label: t("按代码内容搜索提交"), keys: "daima dm neirong code content search pickaxe", run: () => toolbar?.focusSearch("code") },
        { group: g, label: t("按日期搜索提交"), keys: "riqi rq shijian date time search", run: () => toolbar?.focusSearch("date") },
        {
          group: g,
          label: tb.focus.length ? t("显示全部分支") : t("只看当前分支"),
          keys: "zhikan dangqian fenzhi zkdqfz xianshi quanbu xsqb focus current all branches",
          run: () => tb.setFocus(tb.focus.length ? [] : ["HEAD"]),
        },
        { group: g, label: t("用编辑器打开"), keys: "bianjiqi bjq editor code open", run: () => openIn(tb, "editor") },
        { group: g, label: t("在终端中打开"), keys: "zhongduan zd terminal shell", run: () => openIn(tb, "terminal") },
        {
          group: g,
          label: isMac ? t("在访达中显示") : t("在资源管理器中显示"),
          keys: "fangda fd ziyuan guanliqi finder explorer folder",
          run: () => openIn(tb, "folder"),
        },
        { group: g, label: t("查看 git 命令和输出日志"), keys: "rizhi rz log output", run: () => (tb.logOpen = true) },
        { group: g, label: t("关闭标签页"), keys: "guanbi gb close tab", run: () => closeTab(tb) },
        ...tb.remotes.flatMap((r): Item[] => [
          {
            group: g,
            label: tb.remotes.length > 1 ? t("复制 {0} 的地址", r.name) : t("复制仓库地址"),
            hint: shareUrl(r.url),
            keys: `fuzhi dizhi fzdz cangku copy url remote address ${r.name}`,
            run: () => copyRemoteUrl(r),
          },
          {
            group: g,
            label: tb.remotes.length > 1 ? t("在浏览器中打开 {0}", r.name) : t("在浏览器中打开仓库"),
            keys: `liulanqi llq wangye open browser web gitlab github gitee ${r.name}`,
            run: () => openRemoteWeb(r),
          },
        ]),
        { group: g, label: t("添加远程仓库…"), keys: "tianjia yuancheng tjyc add remote upstream", run: () => addRemote(tb) },
        ...(tb.defaultRemote
          ? [{ group: g, label: t("推送所有标签到 {0}", tb.defaultRemote.name), keys: "tuisong biaoqian tsbq push tags", run: () => tb.run({ op: "push_tag", remote: tb.defaultRemote!.name, tag: null }, t("正在推送标签…")) }]
          : []),
        { group: g, label: t("找回丢失的提交（Reflog）…"), keys: "zhaohui diushi tijiao zhdstj reflog recover lost commit", run: () => tb.openTool("reflog") },
        ...(tb.remotes.length
          ? [
              { group: g, label: t("Pull Request / Merge Request"), keys: "pr mr hebing qingqiu hbqq pull request merge", run: () => tb.openTool("pulls") },
              { group: g, label: t("为当前分支创建 PR…"), keys: "chuangjian cj pr mr create pull request", run: () => createPull(tb) },
            ]
          : []),
        { group: g, label: t("新建工作树…"), keys: "gongzuoshu gzs worktree", run: () => addWorktree(tb) },
        { group: g, label: t("添加子模块…"), keys: "zimokuai zmk submodule add", run: () => addSubmodule(tb) },
        ...(tb.extras?.submodules.length
          ? [{ group: g, label: t("更新全部子模块"), keys: "gengxin zimokuai gxzmk submodule update", run: () => tb.run({ op: "submodule_update", path: null }, t("正在更新子模块…")) }]
          : []),
        tb.bisect?.active
          ? { group: g, label: t("结束二分查错"), keys: "erfen chacuo efcc bisect reset", run: () => tb.run({ op: "bisect_reset" }, t("正在结束二分查错…")) }
          : { group: g, label: t("开始二分查错…"), keys: "erfen chacuo efcc bisect start bug", run: () => startBisect(tb) },
        ...(tb.extras?.lfs.installed
          ? [
              { group: g, label: t("Git LFS：拉取大文件"), keys: "lfs dawenjian dwj pull large", run: () => tb.run({ op: "lfs_pull" }, t("正在拉取 LFS 文件…")) },
              { group: g, label: t("Git LFS：管理一类文件…"), keys: "lfs track", run: () => trackWithLfs(tb, null) },
            ]
          : []),
        { group: g, label: t("提交身份…"), hint: tb.extras?.identity.email ?? undefined, keys: "tijiao shenfen tjsf youxiang yx identity email user name author", run: () => (app.identityDialog = tb) },
      );
      if (tb.flow?.initialized) {
        const kinds: FlowKind[] = ["feature", "bugfix", "release", "hotfix"];
        for (const kind of kinds)
          list.push({ group: "Git Flow", label: t("开始 {0}", kind), keys: `kaishi ks git flow start ${kind}`, run: () => openFlow({ tab: tb, mode: "start", kind }) });
        const cur = tb.flowBranch(repo.head.branch);
        if (cur)
          list.push({
            group: "Git Flow",
            label: t("完成 {0}", repo.head.branch ?? ""),
            keys: "wancheng wc git flow finish",
            run: () => openFlow({ tab: tb, mode: "finish", kind: cur.kind, name: cur.name }),
          });
      } else {
        list.push({ group: "Git Flow", label: t("初始化 Git Flow…"), keys: "chushihua csh git flow init", run: () => openFlow({ tab: tb, mode: "init" }) });
      }
    }
    list.push(
      { group: g, label: t("打开本地仓库…"), hint: `${mod}O`, keys: "dakai cangku dkck open repository", run: onOpenFolder },
      { group: g, label: t("克隆远程仓库…"), keys: "kelong kl clone", run: () => (app.cloneDialog = { mode: "clone" }) },
      { group: g, label: t("新建仓库…"), keys: "xinjian cangku xjck new init repository", run: () => (app.cloneDialog = { mode: "init" }) },
      { group: g, label: t("任务面板"), keys: "renwu rw tasks board kanban", run: () => showTasks() },
      { group: g, label: t("提醒与通知…"), keys: "tixing tx tongzhi tz reminders notifications heshui", run: () => openSettings("notify") },
      { group: g, label: t("设置…"), hint: `${mod},`, keys: "shezhi sz settings preferences", run: () => openSettings() },
      { group: g, label: t("检查更新…"), keys: "jiancha gengxin jcgx check update", run: () => checkUpdate(true) },
      { group: g, label: t("主题：跟随系统"), keys: "zhuti zt xitong system theme", run: () => setTheme("system") },
      ...[...PRESETS, ...themes.custom].map((th) => ({ group: g, label: t("主题：{0}", th.name), keys: `zhuti zt theme ${th.id} ${th.name}`, run: () => setTheme(th.id) })),
      lang === "zh"
        ? { group: g, label: "Switch to English", keys: "yuyan yy yingwen language english", run: () => setLang("en") }
        : { group: g, label: "切换到中文", keys: "yuyan yy zhongwen language chinese", run: () => setLang("zh") },
    );
    return list;
  }

  /** 分支、标签、仓库 */
  function targets(): Item[] {
    const list: Item[] = [];
    if (tab && repo) {
      const tb = tab;
      const gb = t("分支");
      for (const r of repo.refs) {
        if (r.kind === "local" && !r.is_head)
          list.push({ group: gb, label: r.name, hint: t("切换"), run: () => tb.run({ op: "checkout", name: r.name }, t("正在切换到 {0}…", r.name)) });
        else if (r.kind === "remote" && !r.name.endsWith("/HEAD")) {
          const short = r.name.slice(r.name.indexOf("/") + 1);
          const local = repo.refs.find((x) => x.kind === "local" && x.name === short);
          if (!local)
            list.push({
              group: gb,
              label: r.name,
              hint: t("检出"),
              run: () => tb.run({ op: "checkout_remote", remote: r.name, local: short }, t("正在检出…")),
            });
        } else if (r.kind === "tag")
          list.push({ group: t("标签"), label: r.name, hint: t("在图中定位"), run: () => tb.jumpToOid(r.oid) });
      }
    }
    const gr = t("仓库");
    for (const x of app.tabs)
      if (x.id && (x !== tab || app.view !== "repo")) list.push({ group: gr, label: x.name, hint: x.id, run: () => activate(x) });
    for (const p of recentRepos())
      if (!app.tabs.some((x) => x.id === p)) list.push({ group: gr, label: repoName(p), hint: p, run: () => openRepo(p) });
    return list;
  }

  const all = [...commands(), ...targets()];
  /** 不输入时只列命令，分支可能有上千个 */
  const LIMIT = 60;

  let query = $state("");
  let index = $state(0);
  let input = $state<HTMLInputElement>();
  let listEl = $state<HTMLDivElement>();

  const results = $derived.by(() => {
    const q = query.trim();
    if (!q) return all.filter((x) => x.group === t("命令") || x.group === "Git Flow").slice(0, LIMIT);
    return all
      .map((x) => ({ x, s: bestScore(q, [x.label, x.keys ?? ""]) }))
      .filter((r) => r.s >= 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, LIMIT)
      .map((r) => r.x);
  });

  $effect(() => {
    void query;
    index = 0;
  });
  $effect(() => {
    input?.focus();
  });
  $effect(() => {
    listEl?.querySelector(`[data-i="${index}"]`)?.scrollIntoView({ block: "nearest" });
  });

  function run(item: Item | undefined) {
    if (!item) return;
    onClose();
    item.run();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
    else if (e.key === "ArrowDown") {
      e.preventDefault();
      index = Math.min(results.length - 1, index + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      index = Math.max(0, index - 1);
    } else if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      run(results[index]);
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={onClose}>
  <div class="palette" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <input bind:this={input} bind:value={query} onkeydown={onKey} placeholder={t("输入命令、分支或仓库名，支持拼音首字母")} spellcheck="false" />
    <div class="list" bind:this={listEl}>
      {#each results as item, i (i)}
        {#if !query.trim() && (i === 0 || results[i - 1].group !== item.group)}<div class="group">{item.group}</div>{/if}
        <div class="item" class:sel={i === index} data-i={i} onmousemove={() => (index = i)} onclick={() => run(item)}>
          {#if query.trim()}<span class="tag">{item.group}</span>{/if}
          <span class="label">{item.label}</span>
          {#if item.hint}<span class="hint">{item.hint}</span>{/if}
        </div>
      {:else}
        <div class="empty">{t("没有匹配的命令")}</div>
      {/each}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 130;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 12vh;
    background: rgba(0, 0, 0, 0.3);
  }
  .palette {
    display: flex;
    flex-direction: column;
    width: 600px;
    max-width: calc(100vw - 32px);
    max-height: 60vh;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  input {
    height: 46px;
    padding: 0 16px;
    border: 0;
    border-bottom: 1px solid var(--border);
    border-radius: 0;
    background: none;
    font-size: 15px;
  }
  input:focus {
    outline: none;
    box-shadow: none;
  }
  .list {
    overflow: auto;
    padding: 4px 0 8px;
  }
  .group {
    padding: 8px 16px 4px;
    color: var(--text-faint);
    font-size: 11.5px;
    font-weight: 600;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 32px;
    padding: 0 16px;
    font-size: 13px;
    cursor: default;
  }
  .item.sel {
    background: var(--accent);
    color: #fff;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .hint {
    max-width: 50%;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--text-faint);
    font-size: 12px;
  }
  .tag {
    flex-shrink: 0;
    min-width: 52px;
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .item.sel .tag,
  .item.sel .hint {
    color: rgba(255, 255, 255, 0.75);
  }
  .empty {
    padding: 24px;
    color: var(--text-faint);
    text-align: center;
  }
</style>
