<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, type MergeMode } from "../lib/api";
  import { savedLang, setLang, t, type Lang } from "../lib/i18n";
  import { prefs } from "../lib/prefs";
  import { activeTab, app, errorText, setAutoFetch, setEditor, setTheme, toast, type SettingsSection, type Theme } from "../lib/store.svelte";
  import { checkUpdate, updater } from "../lib/updater.svelte";
  import NotifySettings from "./NotifySettings.svelte";

  const SECTIONS: { id: SettingsSection; label: string; icon: string }[] = [
    { id: "appearance", label: t("外观"), icon: "◐" },
    { id: "git", label: "Git", icon: "⑂" },
    { id: "identity", label: t("提交身份"), icon: "☺" },
    { id: "tools", label: t("外部工具"), icon: "↗" },
    { id: "notify", label: t("提醒与通知"), icon: "⏰" },
    { id: "about", label: t("关于"), icon: "ⓘ" },
  ];
  const section = $derived(SECTIONS.find((s) => s.id === app.settings) ?? SECTIONS[0]);

  function close() {
    app.settings = null;
  }

  // ---------- 外观 ----------
  const THEMES: { value: Theme; label: string }[] = [
    { value: "dark", label: t("深色") },
    { value: "light", label: t("浅色") },
    { value: "system", label: t("跟随系统") },
  ];
  /** 语言名用它自己的语言写，看不懂当前界面也能找到 */
  const LANGS: { value: Lang | null; label: string }[] = [
    { value: null, label: t("跟随系统语言") },
    { value: "zh", label: "中文" },
    { value: "en", label: "English" },
  ];
  function changeLang(l: Lang | null) {
    if (savedLang() === l) return;
    // 切换语言会重新加载界面，正在执行的操作的进度就看不到了
    if (app.tabs.some((x) => x.busy)) return toast(t("有操作正在进行，完成后再切换语言"), "error");
    setLang(l);
  }

  // ---------- Git ----------
  const AUTO_FETCH = [0, 5, 10, 15, 30, 60];
  const MERGE_MODES: { value: MergeMode; label: string }[] = [
    { value: "auto", label: t("自动") },
    { value: "no_ff", label: t("总是创建合并提交") },
    { value: "ff_only", label: t("只允许快进") },
    { value: "squash", label: t("压缩成一个提交") },
  ];
  let cloneDir = $state(prefs.cloneDir());
  let mergeMode = $state(prefs.mergeMode());
  let diffSplit = $state(prefs.diffSplit());

  async function pickCloneDir() {
    const dir = await openDialog({ directory: true, defaultPath: cloneDir || undefined, title: t("克隆和新建仓库默认放在哪里") });
    if (typeof dir !== "string") return;
    cloneDir = dir;
    prefs.setCloneDir(dir);
  }

  // ---------- 提交身份 ----------
  let idName = $state("");
  let idEmail = $state("");
  let idSaved = $state({ name: "", email: "" });
  let idLoaded = $state(false);
  $effect(() => {
    if (section.id !== "identity" || idLoaded) return;
    api
      .globalIdentity()
      .then((id) => {
        idName = id.name ?? "";
        idEmail = id.email ?? "";
        idSaved = { name: idName, email: idEmail };
        idLoaded = true;
      })
      .catch((e) => toast(errorText(e), "error"));
  });
  const idChanged = $derived(idName.trim() !== idSaved.name || idEmail.trim() !== idSaved.email);

  async function saveIdentity() {
    try {
      await api.setGlobalIdentity(idName, idEmail);
      idSaved = { name: idName.trim(), email: idEmail.trim() };
      toast(t("已保存全局提交身份"));
      // 当前仓库显示的身份可能来自全局配置
      activeTab()?.loadExtras();
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  function manageProfiles() {
    const tab = activeTab();
    if (!tab?.repo) return;
    close();
    app.identityDialog = tab;
  }

  // ---------- 外部工具 ----------
  const EDITORS = [
    { cmd: "code", label: "VS Code" },
    { cmd: "cursor", label: "Cursor" },
    { cmd: "zed", label: "Zed" },
    { cmd: "idea", label: "IntelliJ IDEA" },
    { cmd: "webstorm", label: "WebStorm" },
    { cmd: "subl", label: "Sublime Text" },
  ];
  let editor = $state(app.editor);

  const HOME = "https://github.com/ADroidCoder/GItTree";
</script>

<!-- 上面还有确认框等对话框时，Esc 先交给它们 -->
<svelte:window onkeydown={(e) => e.key === "Escape" && !app.dialog && !updater.open && close()} />

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div class="overlay" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="dialog" role="dialog" aria-modal="true">
    <nav>
      <div class="ntitle">{t("设置")}</div>
      {#each SECTIONS as s (s.id)}
        <button class="nav" class:on={s.id === section.id} onclick={() => (app.settings = s.id)}>
          <span class="nico">{s.icon}</span>{s.label}
        </button>
      {/each}
    </nav>

    <div class="main">
      <div class="head">
        <span class="title">{section.label}</span>
        <button class="x" onclick={close} title={t("关闭 (Esc)")}>✕</button>
      </div>

      <div class="body">
        {#if section.id === "appearance"}
          <div class="item">
            <div class="label">{t("主题")}</div>
            <div class="seg">
              {#each THEMES as th (th.value)}
                <button class:on={app.theme === th.value} onclick={() => setTheme(th.value)}>{th.label}</button>
              {/each}
            </div>
          </div>
          <div class="item">
            <div class="label">
              {t("语言")}
              <div class="hint">{t("切换后界面会重新加载")}</div>
            </div>
            <select value={savedLang() ?? ""} onchange={(e) => changeLang((e.currentTarget.value || null) as Lang | null)}>
              {#each LANGS as l (l.label)}<option value={l.value ?? ""}>{l.label}</option>{/each}
            </select>
          </div>
        {:else if section.id === "git"}
          <div class="item">
            <div class="label">
              {t("自动获取")}
              <div class="hint">{t("定时在后台获取所有远程仓库的更新，不会修改本地分支")}</div>
            </div>
            <select value={app.autoFetch} onchange={(e) => setAutoFetch(Number(e.currentTarget.value))}>
              {#each AUTO_FETCH as m (m)}<option value={m}>{m ? t("每 {0} 分钟", m) : t("关闭")}</option>{/each}
            </select>
          </div>
          <div class="item">
            <div class="label">
              {t("默认克隆位置")}
              <div class="hint path">{cloneDir || t("未设置，每次克隆时选择")}</div>
            </div>
            <button onclick={pickCloneDir}>{t("选择…")}</button>
          </div>
          <div class="item">
            <div class="label">
              {t("默认合并方式")}
              <div class="hint">{t("合并对话框里默认选中的方式；每次合并时也会记住你的选择")}</div>
            </div>
            <select bind:value={mergeMode} onchange={() => prefs.setMergeMode(mergeMode)}>
              {#each MERGE_MODES as m (m.value)}<option value={m.value}>{m.label}</option>{/each}
            </select>
          </div>
          <div class="item">
            <div class="label">{t("差异显示方式")}</div>
            <div class="seg">
              <button class:on={!diffSplit} onclick={() => prefs.setDiffSplit((diffSplit = false))}>{t("上下")}</button>
              <button class:on={diffSplit} onclick={() => prefs.setDiffSplit((diffSplit = true))}>{t("左右并排")}</button>
            </div>
          </div>
        {:else if section.id === "identity"}
          <div class="intro">{t("全局身份写在 ~/.gitconfig 里，没有单独设置的仓库提交时都用它。")}</div>
          <div class="item">
            <div class="label">{t("用户名")}</div>
            <input class="wide" bind:value={idName} placeholder={t("例如：张三")} disabled={!idLoaded} />
          </div>
          <div class="item">
            <div class="label">{t("邮箱")}</div>
            <input class="wide" bind:value={idEmail} placeholder="name@example.com" disabled={!idLoaded} />
          </div>
          <div class="item end">
            <button class="primary" disabled={!idChanged} onclick={saveIdentity}>{t("保存")}</button>
          </div>
          <div class="item">
            <div class="label">
              {t("保存的身份与目录规则")}
              <div class="hint">{t("给某个仓库单独设置身份，或者让某个文件夹下的仓库自动用公司邮箱")}</div>
            </div>
            <button disabled={!activeTab()?.repo} title={activeTab()?.repo ? "" : t("先打开一个仓库")} onclick={manageProfiles}>{t("管理…")}</button>
          </div>
        {:else if section.id === "tools"}
          <div class="item">
            <div class="label">
              {t("编辑器")}
              <div class="hint">{t("用来打开仓库和文件的命令，需要已经加到 PATH 里")}</div>
            </div>
            <input class="cmd" list="editors" bind:value={editor} onchange={() => setEditor(editor)} placeholder="code" />
            <datalist id="editors">
              {#each EDITORS as ed (ed.cmd)}<option value={ed.cmd}>{ed.label}</option>{/each}
            </datalist>
          </div>
          <div class="chips">
            {#each EDITORS as ed (ed.cmd)}
              <button class="chip" class:on={app.editor === ed.cmd} onclick={() => setEditor((editor = ed.cmd))}>{ed.label}</button>
            {/each}
          </div>
        {:else if section.id === "notify"}
          <NotifySettings />
        {:else}
          <div class="about">
            <div class="app">GitTree</div>
            <div class="ver">{updater.version ? `v${updater.version}` : ""}</div>
            <div class="btns">
              {#if updater.update}
                <button class="primary" onclick={() => (updater.open = true)}>{t("安装新版本 {0}…", updater.update.version)}</button>
              {:else}
                <button disabled={updater.status === "checking"} onclick={() => checkUpdate(true)}>
                  {updater.status === "checking" ? t("正在检查…") : t("检查更新")}
                </button>
              {/if}
              <button onclick={() => api.openUrl(HOME).catch((e) => toast(errorText(e), "error"))}>{t("项目主页")}</button>
            </div>
            <div class="hint">{t("每 6 小时会在后台自动检查一次新版本")}</div>
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 95;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 7vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    display: flex;
    width: 780px;
    max-width: calc(100vw - 32px);
    height: min(600px, 86vh);
    overflow: hidden;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
    font-size: 13px;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 180px;
    flex-shrink: 0;
    padding: 14px 8px;
    background: var(--bg-side);
    border-right: 1px solid var(--border);
  }
  .ntitle {
    padding: 0 10px 10px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 8px;
    border: 0;
    background: none;
    text-align: left;
  }
  .nav:hover {
    background: var(--bg-hover);
  }
  .nav.on {
    background: var(--bg-selected);
    font-weight: 600;
  }
  .nico {
    width: 16px;
    color: var(--text-dim);
    text-align: center;
  }
  .main {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: center;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
  }
  .title {
    flex: 1;
    font-size: 15px;
    font-weight: 600;
  }
  .x {
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-dim);
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 18px 18px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 0;
    border-bottom: 1px solid var(--border);
  }
  .item.end {
    justify-content: flex-end;
    border-bottom: 0;
  }
  .label {
    flex: 1;
    min-width: 0;
  }
  .hint {
    margin-top: 2px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .hint.path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .intro {
    padding: 12px 0 4px;
    color: var(--text-dim);
  }
  select {
    height: 28px;
    min-width: 160px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
  }
  .wide {
    width: 280px;
  }
  .cmd {
    width: 160px;
  }
  .seg {
    display: flex;
  }
  .seg button {
    border-radius: 0;
  }
  .seg button + button {
    margin-left: -1px;
  }
  .seg button:first-child {
    border-radius: 5px 0 0 5px;
  }
  .seg button:last-child {
    border-radius: 0 5px 5px 0;
  }
  .seg button.on,
  .chip.on {
    position: relative;
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding-top: 12px;
  }
  .chip {
    height: 26px;
    border-radius: 13px;
    font-size: 12px;
  }
  .about {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding-top: 60px;
  }
  .app {
    font-size: 20px;
    font-weight: 700;
  }
  .ver {
    color: var(--text-dim);
  }
  .btns {
    display: flex;
    gap: 8px;
    margin: 8px 0;
  }
</style>
