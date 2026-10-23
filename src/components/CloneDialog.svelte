<script lang="ts">
  import { t } from "../lib/i18n";
  import { notifyInBackground, notifySettings } from "../lib/notify.svelte";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, type ProgressEvent } from "../lib/api";
  import { activeTab, app, elapsed, errorText, openRepo } from "../lib/store.svelte";
  import { prefs } from "../lib/prefs";

  // svelte-ignore state_referenced_locally
  let mode = $state(app.cloneDialog!.mode);
  let url = $state("");
  let parent = $state(prefs.cloneDir());
  let name = $state("");
  /** 用户改过文件夹名后就不再跟着地址自动变化 */
  let nameEdited = $state(false);
  let running = $state(false);
  let since = $state(0);
  let now = $state(Date.now());
  let lines = $state<string[]>([]);
  let error = $state<string | null>(null);
  let urlInput = $state<HTMLInputElement>();
  /** 大仓库：浅克隆、部分检出 */
  let advanced = $state(false);
  let shallow = $state(false);
  let depth = $state(50);
  let sparse = $state(false);
  let sparseDirs = $state("");

  /** 从地址里取仓库名：https://gitee.com/a/b.git、git@github.com:a/b.git → b */
  function nameFromUrl(u: string) {
    return (
      u
        .trim()
        .replace(/[?#].*$/, "")
        .replace(/\/+$/, "")
        .split(/[/:\\]/)
        .pop()
        ?.replace(/\.git$/, "") ?? ""
    );
  }

  $effect(() => {
    if (!nameEdited && mode === "clone") name = nameFromUrl(url);
  });

  $effect(() => {
    if (!running) return;
    const t = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(t);
  });

  onMount(() => {
    urlInput?.focus();
    // 克隆的进度事件用 "clone" 作为标识
    const un = listen<ProgressEvent>("git-progress", (e) => {
      if (e.payload.repo !== "clone") return;
      const last = lines[lines.length - 1];
      const stage = (t: string) => t.replace(/^remote:\s*/, "").split(":")[0];
      if (last !== undefined && stage(last) === stage(e.payload.text) && /\d+%/.test(last)) lines[lines.length - 1] = e.payload.text;
      else lines.push(e.payload.text);
      if (lines.length > 300) lines.splice(0, lines.length - 300);
    });
    return () => {
      un.then((f) => f());
    };
  });

  async function pickParent() {
    const dir = await openDialog({ directory: true, title: t("选择保存位置"), defaultPath: parent || undefined });
    if (typeof dir === "string") parent = dir;
  }

  /** Windows 路径用反斜杠 */
  const sep = $derived(parent.includes("\\") ? "\\" : "/");
  const target = $derived(parent && name.trim() ? `${parent.replace(/[\\/]+$/, "")}${sep}${name.trim()}` : "");
  const canStart = $derived(!running && !!target && (mode === "init" || !!url.trim()));

  async function start() {
    if (!canStart) return;
    prefs.setCloneDir(parent);
    running = true;
    since = Date.now();
    error = null;
    lines = [];
    try {
      if (mode === "clone") {
        const dirs = sparseDirs.split(/[\s,，]+/).filter(Boolean);
        const path = await api.cloneRepo(url, parent, name, {
          depth: shallow ? Math.max(1, Math.floor(depth) || 1) : null,
          sparse: sparse ? dirs : null,
        });
        if (notifySettings.gitOps) notifyInBackground(t("克隆完成"), path);
        close();
        await openRepo(path);
        // 部分检出时没填目录：打开后直接让用户勾选
        const tab = activeTab();
        if (sparse && !dirs.length && tab?.id === path) app.sparseDialog = tab;
      } else {
        await api.initRepo(target);
        close();
        await openRepo(target);
      }
    } catch (e) {
      error = errorText(e);
    } finally {
      running = false;
    }
  }

  function close() {
    app.cloneDialog = null;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !running) close();
    if (e.key === "Enter" && !e.isComposing && (e.target as HTMLElement).tagName === "INPUT") start();
  }

  /** 最新一行进度里的百分比 */
  const percent = $derived.by(() => {
    const m = lines[lines.length - 1]?.match(/(\d+)%/);
    return m ? Number(m[1]) : null;
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={onKey}>
  <div class="dialog" role="dialog" aria-modal="true">
    <div class="tabs">
      <button class:on={mode === "clone"} disabled={running} onclick={() => (mode = "clone")}>{t("克隆远程仓库")}</button>
      <button class:on={mode === "init"} disabled={running} onclick={() => (mode = "init")}>{t("新建本地仓库")}</button>
    </div>

    <div class="grid">
      {#if mode === "clone"}
        <label for="c-url">{t("仓库地址")}</label>
        <input id="c-url" bind:this={urlInput} bind:value={url} disabled={running} placeholder={t("https://gitee.com/xxx/yyy.git 或 git@gitee.com:xxx/yyy.git")} spellcheck="false" />
      {/if}
      <label for="c-parent">{t("保存到")}</label>
      <div class="row">
        <input id="c-parent" bind:value={parent} disabled={running} placeholder={t("选择一个文件夹")} spellcheck="false" />
        <button disabled={running} onclick={pickParent}>{t("选择…")}</button>
      </div>
      <label for="c-name">{t("文件夹名")}</label>
      <input id="c-name" bind:value={name} oninput={() => (nameEdited = true)} disabled={running} spellcheck="false" />
    </div>
    {#if target}<div class="hint">{mode === "clone" ? t("将克隆到 {0}", target) : t("将创建到 {0}", target)}</div>{/if}
    {#if mode === "clone" && !running && !error}
      <div class="hint">{t("需要账号密码的 HTTPS 地址，请先在系统的 Git 凭据管理器里登录过，或者改用 SSH 地址。")}</div>
    {/if}
    {#if mode === "clone"}
      <button class="adv-toggle" class:open={advanced} disabled={running} onclick={() => (advanced = !advanced)}>
        <span class="arrow">▸</span>{t("大仓库选项")}
        {#if !advanced && (shallow || sparse)}<span class="adv-on">{[shallow && t("浅克隆"), sparse && t("部分检出")].filter(Boolean).join(" · ")}</span>{/if}
      </button>
      {#if advanced}
        <div class="adv">
          <label class="opt">
            <input type="checkbox" bind:checked={shallow} disabled={running} />
            <span>{t("只下载最近的历史（浅克隆）：每个分支最近")}</span>
            <input class="num" type="number" min="1" bind:value={depth} disabled={running || !shallow} />
            <span>{t("个提交")}</span>
          </label>
          <div class="hint sub">{t("历史很长的仓库能快很多。以后可以在「更多」菜单里补下载更早的历史。")}</div>
          <label class="opt">
            <input type="checkbox" bind:checked={sparse} disabled={running} />
            <span>{t("只检出部分目录（sparse-checkout）")}</span>
          </label>
          {#if sparse}
            <input class="dirs" bind:value={sparseDirs} disabled={running} placeholder={t("要检出的目录，多个用空格分开，例如 src docs；留空则克隆后再选")} spellcheck="false" />
          {/if}
          <div class="hint sub">{t("其他目录不检出到硬盘，文件内容也是用到时才下载。以后可以在「更多」菜单里改选目录。")}</div>
        </div>
      {/if}
    {/if}

    {#if running || lines.length || error}
      <div class="progress">
        {#if running}
          <div class="ptitle">
            <span class="spinner"></span>
            {mode === "clone" ? t("正在克隆…") : t("正在创建…")} <span class="time">{now && elapsed(since)}</span>
          </div>
          {#if percent !== null}<div class="bar"><div style="width:{percent}%"></div></div>{/if}
        {/if}
        {#if error}<div class="error">{error}</div>{/if}
        {#if lines.length}
          <div class="log">
            {#each lines as l, i (i)}<div class:cmd={l.startsWith("$ ")}>{l}</div>{/each}
          </div>
        {/if}
      </div>
    {/if}

    <div class="buttons">
      {#if running && mode === "clone"}
        <button onclick={() => api.cancelOp("clone")}>{t("取消克隆")}</button>
      {:else}
        <button onclick={close} disabled={running}>{t("关闭")}</button>
      {/if}
      <button class="primary" disabled={!canStart} onclick={start}>{mode === "clone" ? t("克隆") : t("创建")}</button>
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
    padding-top: 10vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 600px;
    max-width: calc(100vw - 32px);
    padding: 16px 20px 18px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
  }
  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--border);
  }
  .tabs button {
    height: 32px;
    border: 0;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    background: none;
    color: var(--text-dim);
  }
  .tabs button.on {
    border-bottom-color: var(--accent);
    color: var(--text);
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: 70px 1fr;
    align-items: center;
    gap: 10px 12px;
    font-size: 13px;
  }
  .grid label {
    color: var(--text-dim);
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .row input {
    flex: 1;
    min-width: 0;
  }
  .hint {
    color: var(--text-faint);
    font-size: 12px;
    word-break: break-all;
  }
  .adv-toggle {
    align-self: flex-start;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-dim);
    font-size: 12.5px;
  }
  .adv-toggle .arrow {
    font-size: 10px;
    transition: transform 0.12s;
  }
  .adv-toggle.open .arrow {
    transform: rotate(90deg);
  }
  .adv-on {
    color: var(--accent);
  }
  .adv {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    font-size: 13px;
  }
  .opt {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .num {
    width: 72px;
    height: 24px;
  }
  .dirs {
    margin-left: 22px;
  }
  .hint.sub {
    margin: -2px 0 4px 22px;
    word-break: normal;
  }
  .progress {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ptitle {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }
  .time {
    color: var(--text-faint);
  }
  .bar {
    height: 3px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .error {
    color: var(--del);
    font-size: 12.5px;
    white-space: pre-wrap;
  }
  .log {
    max-height: 180px;
    overflow: auto;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--text-dim);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .log .cmd {
    color: var(--accent);
  }
  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
