<script lang="ts">
  import { syncBackendLang, t } from "./lib/i18n";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, type AskPassRequest, type ProgressEvent } from "./lib/api";
  import { activate, activeTab, app, openRepo, preloadTasks, restoreTabs, startAutoFetch, tabById } from "./lib/store.svelte";
  import { isMac } from "./lib/util";
  import TabBar from "./components/TabBar.svelte";
  import Toolbar from "./components/Toolbar.svelte";
  import RepoView from "./components/RepoView.svelte";
  import Dialog from "./components/Dialog.svelte";
  import FlowDialog from "./components/FlowDialog.svelte";
  import Welcome from "./components/Welcome.svelte";
  import TaskBoard from "./components/TaskBoard.svelte";
  import OpLog from "./components/OpLog.svelte";
  import CloneDialog from "./components/CloneDialog.svelte";
  import RebaseDialog from "./components/RebaseDialog.svelte";
  import AskPass from "./components/AskPass.svelte";
  import UpdateDialog from "./components/UpdateDialog.svelte";
  import { startUpdater, updater } from "./lib/updater.svelte";
  import NotifyDialog from "./components/NotifyDialog.svelte";
  import CommandPalette from "./components/CommandPalette.svelte";
  import { notifyDialog, startNotifier } from "./lib/notify.svelte";

  let toolbar = $state<ReturnType<typeof Toolbar>>();
  const tab = $derived(activeTab());

  async function openFolder() {
    const dir = await openDialog({ directory: true, title: t("选择 Git 仓库所在的文件夹") });
    if (typeof dir === "string") await openRepo(dir);
  }

  onMount(() => {
    syncBackendLang();
    startUpdater();
    startNotifier();
    // 恢复上次打开的标签页；命令行指定的仓库额外打开
    api.initialRepo().then((p) => restoreTabs(p));
    preloadTasks();
    const stopAutoFetch = startAutoFetch();
    const unlisten = [
      listen<string>("repo-changed", (e) => tabById(e.payload)?.refresh()),
      listen<string>("workdir-changed", (e) => tabById(e.payload)?.refreshStatus()),
      listen<AskPassRequest>("askpass", (e) => app.askpass.push(e.payload)),
      listen<ProgressEvent>("git-progress", (e) => tabById(e.payload.repo)?.onProgress(e.payload)),
    ];
    // 切回窗口时刷新当前标签页（可能在终端里改过仓库）
    const onFocus = () => activeTab()?.refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      unlisten.forEach((p) => p.then((f) => f()));
      stopAutoFetch();
      window.removeEventListener("focus", onFocus);
    };
  });

  function isEditable(t: EventTarget | null) {
    const el = t as HTMLElement | null;
    return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable);
  }

  let paletteOpen = $state(false);

  function onGlobalKey(e: KeyboardEvent) {
    const mod = isMac ? e.metaKey : e.ctrlKey;
    const key = e.key.toLowerCase();
    if (mod && (key === "k" || (key === "p" && e.shiftKey))) {
      // ⌘K / ⌘⇧P 命令面板
      e.preventDefault();
      paletteOpen = !paletteOpen;
    } else if (mod && key === "f") {
      e.preventDefault();
      toolbar?.focusSearch();
    } else if ((mod && key === "r") || e.key === "F5") {
      // 阻止整个页面重新加载，改为刷新仓库
      e.preventDefault();
      tab?.refresh();
    } else if (mod && (key === "o" || key === "t")) {
      e.preventDefault();
      openFolder();
    } else if (mod && key === "z" && !e.shiftKey && app.view === "repo" && !isEditable(e.target)) {
      // 输入框里的 ⌘Z 留给输入框自己撤销文字
      e.preventDefault();
      tab?.undo();
    } else if (mod && /^[1-9]$/.test(e.key)) {
      // ⌘1 ~ ⌘9 切换标签页
      const t = app.tabs[Number(e.key) - 1];
      if (t) {
        e.preventDefault();
        activate(t);
      }
    }
  }
</script>

<svelte:window onkeydown={onGlobalKey} />

<div class="app">
  <TabBar onOpenFolder={openFolder} />
  {#if app.view === "repo"}
    <Toolbar bind:this={toolbar} {tab} onOpenFolder={openFolder} />
  {:else}
    <div></div>
  {/if}
  <div class="body">
    {#if app.view === "tasks"}
      <div class="view tasks"><TaskBoard /></div>
    {/if}
    {#each app.tabs as t (t.key)}
      <div class="view" class:hidden={app.view !== "repo" || t.key !== app.activeKey}>
        <RepoView tab={t} active={t.key === app.activeKey} />
      </div>
    {:else}
      {#if app.view === "repo"}<Welcome onOpenFolder={openFolder} />{/if}
    {/each}
  </div>
</div>

{#if tab && (tab.busy || tab.logOpen)}
  <OpLog {tab} />
{/if}
<div class="toasts">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.kind}">{t.text}</div>
  {/each}
</div>
{#if app.askpass.length}
  <AskPass />
{/if}
{#if paletteOpen}
  <CommandPalette {toolbar} onOpenFolder={openFolder} onClose={() => (paletteOpen = false)} />
{/if}
{#if notifyDialog.open}
  <NotifyDialog />
{/if}
{#if updater.open && updater.update}
  <UpdateDialog />
{/if}
{#if app.cloneDialog}
  <CloneDialog />
{/if}
{#if app.rebaseDialog}
  <RebaseDialog />
{/if}
{#if app.flowDialog}
  <FlowDialog />
{/if}
{#if app.dialog}
  <Dialog />
{/if}

<style>
  .app {
    display: grid;
    grid-template-rows: auto auto 1fr;
    height: 100vh;
  }
  .body {
    position: relative;
    min-height: 0;
  }
  .view {
    position: absolute;
    inset: 0;
  }
  /* 隐藏的标签页保持挂载，切回来时滚动位置和缓存都还在 */
  .view.tasks {
    z-index: 1;
  }
  .view.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 460px;
  }
  .toast {
    padding: 10px 14px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-left: 4px solid var(--accent);
    border-radius: 6px;
    box-shadow: var(--shadow);
    font-size: 13px;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 240px;
    overflow: auto;
    user-select: text;
  }
  .toast.error {
    border-left-color: var(--del);
  }
</style>
