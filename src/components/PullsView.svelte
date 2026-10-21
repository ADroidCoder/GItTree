<script lang="ts">
  import { api, type ForgeInfo, type ForgeKind, type PullRequest, type Remote } from "../lib/api";
  import { t } from "../lib/i18n";
  import { copyText, createPull, errorText, toast, type RepoTab } from "../lib/store.svelte";
  import { relativeTime } from "../lib/util";

  let { tab, onClose }: { tab: RepoTab; onClose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let remoteName = $state(tab.defaultRemote?.name ?? "");
  const remote = $derived<Remote | undefined>(tab.remotes.find((r) => r.name === remoteName) ?? tab.defaultRemote);

  let info = $state<ForgeInfo | null>(null);
  let pulls = $state<PullRequest[] | null>(null);
  let error = $state<string | null>(null);
  let needToken = $state(false);
  let loading = $state(false);
  let tokenInput = $state("");
  let showTokenForm = $state(false);
  let filter = $state("");
  let seq = 0;

  const KINDS: { kind: ForgeKind; label: string }[] = [
    { kind: "github", label: "GitHub" },
    { kind: "gitlab", label: "GitLab" },
    { kind: "gitee", label: "Gitee" },
    { kind: "gitea", label: "Gitea / Forgejo" },
  ];
  const isMr = $derived(info?.kind === "gitlab");

  async function load() {
    const r = remote;
    if (!r) return;
    const my = ++seq;
    loading = true;
    error = null;
    needToken = false;
    try {
      info = await api.forgeInfo(r.url);
      if (!info.kind) {
        pulls = null;
        return;
      }
      const res = await api.pullRequests(r.url);
      if (my !== seq) return;
      pulls = res.pulls;
    } catch (e) {
      if (my !== seq) return;
      const msg = errorText(e);
      needToken = msg.startsWith("need_token:");
      error = needToken ? msg.slice("need_token:".length) : msg;
      pulls = null;
      if (needToken) showTokenForm = true;
    } finally {
      if (my === seq) loading = false;
    }
  }

  $effect(() => {
    void remote?.url;
    load();
  });

  async function chooseKind(kind: ForgeKind) {
    if (!info) return;
    await api.forgeConfigure(info.project.host, kind, null).catch((e) => toast(errorText(e), "error"));
    load();
  }

  async function saveToken(clear = false) {
    if (!info) return;
    await api.forgeConfigure(info.project.host, null, clear ? "" : tokenInput.trim()).catch((e) => toast(errorText(e), "error"));
    tokenInput = "";
    showTokenForm = false;
    toast(clear ? t("已删除 {0} 的访问令牌", info.project.host) : t("已保存 {0} 的访问令牌", info.project.host));
    load();
  }

  const shown = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    return (pulls ?? []).filter(
      (p) => !f || [p.title, p.author, p.source, p.target, String(p.number)].some((x) => x.toLowerCase().includes(f)),
    );
  });

  /** 远程上有 PR 的源分支（不是来自 fork 时） */
  function remoteBranch(p: PullRequest) {
    if (p.fork || !remote) return null;
    const name = `${remote.name}/${p.source}`;
    return tab.repo?.refs.find((r) => r.kind === "remote" && r.name === name) ?? null;
  }

  async function checkout(p: PullRequest) {
    const r = remote;
    if (!r || !info?.kind) return;
    const rb = remoteBranch(p);
    if (rb) {
      // 源分支就在这个远程上：检出成同名的本地分支并跟踪它，之后能正常拉取推送
      const local = tab.repo?.refs.find((x) => x.kind === "local" && x.name === p.source);
      if (local) {
        if (!local.is_head) await tab.run({ op: "checkout", name: p.source }, t("正在切换到 {0}…", p.source));
        return;
      }
      await tab.run({ op: "checkout_remote", remote: rb.name, local: p.source }, t("正在检出…"));
      return;
    }
    // 来自 fork：从平台为 PR 准备的引用拉取
    const remoteRef = info.kind === "gitlab" ? `refs/merge-requests/${p.number}/head` : `refs/pull/${p.number}/head`;
    await tab.run(
      { op: "checkout_pull", remote: r.name, remote_ref: remoteRef, local: `pr/${p.number}`, number: p.number },
      t("正在检出 #{0}…", p.number),
    );
  }

  function compare(p: PullRequest) {
    const r = remote;
    if (!r) return;
    const rb = remoteBranch(p);
    if (!rb) return toast(t("PR 来自 fork 仓库，先检出到本地再对比"), "error");
    tab.openCompare(`${r.name}/${p.target}`, rb.name);
  }

  const updated = (s: string) => {
    const ms = Date.parse(s);
    return Number.isNaN(ms) ? s : relativeTime(ms / 1000);
  };
</script>

<div class="pv">
  <div class="bar">
    <span class="title">{isMr ? "Merge Requests" : "Pull Requests"}</span>
    {#if tab.remotes.length > 1}
      <select bind:value={remoteName}>
        {#each tab.remotes as r (r.name)}<option value={r.name}>{r.name}</option>{/each}
      </select>
    {/if}
    {#if info}<span class="proj">{info.project.host}/{info.project.path}</span>{/if}
    <span class="grow"></span>
    {#if info?.kind}
      <input class="filter" bind:value={filter} placeholder={t("筛选")} spellcheck="false" />
      <button onclick={() => createPull(tab)}>{isMr ? t("＋ 创建 MR") : t("＋ 创建 PR")}</button>
      <button onclick={() => info?.pulls_web && api.openUrl(info.pulls_web)} title={t("在浏览器中查看全部")}>↗</button>
      <button onclick={() => (showTokenForm = !showTokenForm)} title={t("设置访问令牌")}>🔑</button>
      <button onclick={load} title={t("刷新")}>↻</button>
    {/if}
    <button onclick={onClose} title={t("关闭 (Esc)")}>✕</button>
  </div>

  {#if !remote}
    <div class="msg">{t("这个仓库还没有配置远程仓库")}</div>
  {:else if info && !info.kind}
    <div class="box">
      <div class="q">{t("{0} 是哪种代码托管平台？", info.project.host)}</div>
      <div class="hint">{t("公司自己部署的平台认不出来，选一次就会记住。")}</div>
      <div class="kinds">
        {#each KINDS as k (k.kind)}<button onclick={() => chooseKind(k.kind)}>{k.label}</button>{/each}
      </div>
    </div>
  {:else}
    {#if showTokenForm && info}
      <div class="box">
        <div class="q">{t("{0} 的访问令牌", info.project.host)}</div>
        <div class="hint">
          {t("私有仓库需要访问令牌才能读取 PR 列表。只需要读取仓库的权限（GitHub：repo 或 Pull requests 只读；GitLab：read_api；Gitee：pull_requests）。令牌只保存在这台电脑上。")}
          {#if info.token_page}
            <button class="link" onclick={() => info?.token_page && api.openUrl(info.token_page)}>{t("去创建令牌 ↗")}</button>
          {/if}
        </div>
        <div class="row">
          <input type="password" bind:value={tokenInput} placeholder={info.has_token ? t("已设置，输入新的令牌可以替换") : t("粘贴访问令牌")} spellcheck="false" />
          <button class="primary" disabled={!tokenInput.trim()} onclick={() => saveToken()}>{t("保存")}</button>
          {#if info.has_token}<button class="danger" onclick={() => saveToken(true)}>{t("删除令牌")}</button>{/if}
        </div>
      </div>
    {/if}
    {#if error}
      <div class="msg error">{error}</div>
    {:else if loading && !pulls}
      <div class="msg">{t("加载中…")}</div>
    {:else if pulls && !shown.length}
      <div class="msg">{pulls.length ? t("没有匹配的") : isMr ? t("没有打开中的 Merge Request") : t("没有打开中的 Pull Request")}</div>
    {:else if pulls}
      <div class="list">
        {#each shown as p (p.number)}
          <div class="pr">
            <div class="main">
              <div class="line1">
                <span class="num">{isMr ? "!" : "#"}{p.number}</span>
                <span class="ptitle">{p.title}</span>
                {#if p.draft}<span class="draft">{t("草稿")}</span>{/if}
              </div>
              <div class="line2">
                {#if p.fork}<span class="draft">fork</span>{/if}
                <span class="br" title={p.source_repo ?? ""}>{p.source_repo ? `${p.source_repo}:` : ""}{p.source}</span>
                → <span class="br">{p.target}</span>
                · {p.author} · {updated(p.updated)}
              </div>
            </div>
            <div class="acts">
              <button onclick={() => checkout(p)} title={t("把 PR 的代码检出到本地")}>{t("检出")}</button>
              <button onclick={() => compare(p)} disabled={!remoteBranch(p)}>{t("对比")}</button>
              <button onclick={() => api.openUrl(p.url)} title={t("在浏览器中打开")}>↗</button>
              <button onclick={() => copyText(p.url)} title={t("复制链接")}>⧉</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<svelte:window onkeydown={(e) => e.key === "Escape" && !(e.target instanceof HTMLInputElement) && onClose()} />

<style>
  .pv {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-graph);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 42px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .title {
    font-weight: 600;
  }
  .proj {
    color: var(--text-faint);
    font-size: 12px;
  }
  select {
    height: 26px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--input-bg);
    color: var(--text);
  }
  .filter {
    width: 160px;
    height: 28px;
  }
  .grow {
    flex: 1;
  }
  .msg {
    padding: 40px;
    text-align: center;
    color: var(--text-dim);
  }
  .msg.error {
    color: var(--del);
    white-space: pre-wrap;
  }
  .box {
    margin: 12px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-side);
  }
  .q {
    font-weight: 600;
    margin-bottom: 4px;
  }
  .hint {
    color: var(--text-dim);
    font-size: 12.5px;
    line-height: 1.6;
  }
  .kinds,
  .row {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  .row input {
    flex: 1;
  }
  .link {
    border: 0;
    background: none;
    color: var(--accent);
    padding: 0 4px;
    height: auto;
  }
  .list {
    flex: 1;
    overflow: auto;
  }
  .pr {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .pr:hover {
    background: var(--bg-hover);
  }
  .main {
    flex: 1;
    min-width: 0;
  }
  .line1 {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .num {
    color: var(--text-faint);
    font-family: var(--mono);
  }
  .ptitle {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-weight: 500;
  }
  .draft {
    padding: 0 6px;
    border-radius: 4px;
    background: var(--bg-hover);
    color: var(--text-faint);
    font-size: 11px;
  }
  .line2 {
    margin-top: 2px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .br {
    font-family: var(--mono);
    color: var(--text-dim);
  }
  .acts {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }
  .acts button {
    height: 26px;
    padding: 0 8px;
    font-size: 12px;
  }
</style>
