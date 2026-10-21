<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, type IdentityProfile } from "../lib/api";
  import { t } from "../lib/i18n";
  import { app, confirm, errorText, toast, type RepoTab } from "../lib/store.svelte";

  let { tab }: { tab: RepoTab } = $props();

  let profiles = $state<IdentityProfile[]>([]);
  /** 正在编辑的身份（id 为空是新建）；local 表示只给本仓库设置 */
  let form = $state<{ mode: "profile" | "local"; id: string | null; label: string; name: string; email: string; key: string } | null>(null);

  async function loadProfiles() {
    profiles = await api.identityProfiles().catch((e) => (toast(errorText(e), "error"), []));
  }
  loadProfiles();

  const id = $derived(tab.extras?.identity);
  /** 当前身份来自哪套保存的身份（目录规则生效时） */
  const fromProfile = $derived(profiles.find((p) => id?.origin && sameFile(p.file, id.origin)));

  function sameFile(a: string, b: string) {
    const n = (s: string) => s.replace(/\\/g, "/").toLowerCase();
    return n(a) === n(b);
  }

  const source = $derived.by(() => {
    if (!id?.email) return t("还没有设置提交身份，提交会失败");
    if (id.scope === "local") return t("本仓库单独设置的（.git/config）");
    if (fromProfile) return t("来自身份「{0}」：这个仓库在它的目录规则里", fromProfile.label);
    if (id.scope === "global") return t("全局设置（{0}）", id.origin ?? "~/.gitconfig");
    return t("来自 {0}", id.origin ?? id.scope ?? "");
  });

  function close() {
    app.identityDialog = null;
  }

  async function after() {
    await Promise.all([loadProfiles(), tab.loadExtras()]);
  }

  async function submit() {
    const f = form;
    if (!f) return;
    if (!f.name.trim() || !f.email.trim()) return toast(t("请填写用户名和邮箱"), "error");
    try {
      if (f.mode === "local") {
        await tab.run({ op: "set_local_identity", name: f.name, email: f.email });
      } else {
        await api.identitySaveProfile(f.id, f.label, f.name, f.email, f.key || null);
      }
      form = null;
      await after();
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  async function addDir(p: IdentityProfile) {
    // 默认从仓库的上一级目录开始选
    const parent = tab.id.replace(/[\\/][^\\/]+[\\/]?$/, "");
    const dir = await openDialog({ directory: true, defaultPath: parent, title: t("在这个文件夹下的仓库都用「{0}」", p.label) });
    if (typeof dir !== "string") return;
    try {
      await api.identitySetDirs(p.id, [...p.dirs, dir]);
      await after();
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  async function removeDir(p: IdentityProfile, dir: string) {
    try {
      await api.identitySetDirs(p.id, p.dirs.filter((d) => d !== dir));
      await after();
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  async function remove(p: IdentityProfile) {
    if (!(await confirm(t("删除身份"), t("确定删除身份「{0}」吗？它的目录规则也会一起删掉。", p.label), t("删除"), true))) return;
    try {
      await api.identityDeleteProfile(p.id);
      await after();
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  const applyHere = (p: IdentityProfile) => tab.run({ op: "set_local_identity", name: p.name, email: p.email }).then(after);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={(e) => e.key === "Escape" && (form ? (form = null) : close())}>
  <div class="dialog" role="dialog" aria-modal="true">
    <div class="head">
      <span class="title">{t("提交身份")}</span>
      <button class="x" onclick={close}>✕</button>
    </div>

    <div class="cur">
      <div class="label">{t("{0} 提交时用的身份", tab.name)}</div>
      <div class="who">{id?.name ?? "—"} <span class="email">&lt;{id?.email ?? "—"}&gt;</span></div>
      <div class="src" class:warn={!id?.email}>{source}</div>
      <div class="btns">
        <button onclick={() => (form = { mode: "local", id: null, label: "", name: id?.name ?? "", email: id?.email ?? "", key: "" })}>
          {t("只给这个仓库设置…")}
        </button>
        {#if id?.scope === "local"}
          <button onclick={() => tab.run({ op: "clear_local_identity" }).then(after)}>{t("清除本仓库的设置")}</button>
        {/if}
      </div>
    </div>

    <div class="sec">
      <span>{t("保存的身份")}</span>
      <button class="small" onclick={() => (form = { mode: "profile", id: null, label: "", name: "", email: "", key: "" })}>{t("＋ 新建身份")}</button>
    </div>
    <div class="list">
      {#each profiles as p (p.id)}
        <div class="profile" class:active={fromProfile?.id === p.id}>
          <div class="ptop">
            <span class="plabel">{p.label}</span>
            <span class="pwho">{p.name} &lt;{p.email}&gt;</span>
            <span class="grow"></span>
            <button class="small" onclick={() => applyHere(p)} title={t("写到这个仓库的 .git/config")}>{t("用于本仓库")}</button>
            <button
              class="small"
              onclick={() => (form = { mode: "profile", id: p.id, label: p.label, name: p.name, email: p.email, key: p.signing_key ?? "" })}>{t("编辑")}</button
            >
            <button class="small danger" onclick={() => remove(p)}>{t("删除")}</button>
          </div>
          <div class="dirs">
            <span class="dlabel">{t("自动用于这些文件夹下的仓库：")}</span>
            {#each p.dirs as d (d)}
              <span class="dir">{d}<button class="dx" onclick={() => removeDir(p, d)} title={t("移除")}>×</button></span>
            {/each}
            <button class="small" onclick={() => addDir(p)}>{t("＋ 添加文件夹")}</button>
          </div>
        </div>
      {:else}
        <div class="empty">
          {t("把公司和个人的用户名、邮箱各存一套，再给它们指定文件夹：例如 ~/work 下的仓库自动用公司邮箱，其他仓库用个人邮箱。")}
        </div>
      {/each}
    </div>

    {#if form}
      <div class="form">
        <div class="ftitle">{form.mode === "local" ? t("只给这个仓库设置") : form.id ? t("编辑身份") : t("新建身份")}</div>
        {#if form.mode === "profile"}
          <input bind:value={form.label} placeholder={t("名称，例如 公司、个人、开源")} />
        {/if}
        <input bind:value={form.name} placeholder={t("用户名 user.name")} spellcheck="false" />
        <input bind:value={form.email} placeholder={t("邮箱 user.email")} spellcheck="false" />
        {#if form.mode === "profile"}
          <input bind:value={form.key} placeholder={t("签名密钥 user.signingkey（可选）")} spellcheck="false" />
        {/if}
        <div class="btns">
          <span class="grow"></span>
          <button onclick={() => (form = null)}>{t("取消")}</button>
          <button class="primary" onclick={submit}>{t("保存")}</button>
        </div>
      </div>
    {/if}

    <div class="foot">
      {t("按文件夹切换用的是 git 自带的 includeIf，写在全局配置 ~/.gitconfig 里，在命令行和其他工具里提交也会生效。")}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 8vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 640px;
    max-width: calc(100vw - 32px);
    max-height: 84vh;
    padding: 16px 20px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
    overflow: auto;
  }
  .head {
    display: flex;
    align-items: center;
  }
  .title {
    flex: 1;
    font-size: 15px;
    font-weight: 600;
  }
  .x {
    border: 0;
    background: none;
  }
  .cur {
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-side);
  }
  .label,
  .dlabel {
    color: var(--text-faint);
    font-size: 12px;
  }
  .who {
    margin: 4px 0 2px;
    font-size: 14px;
    font-weight: 600;
  }
  .email,
  .pwho {
    color: var(--text-dim);
    font-weight: normal;
  }
  .src {
    color: var(--text-dim);
    font-size: 12px;
  }
  .src.warn {
    color: var(--del);
  }
  .btns {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }
  .sec {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-weight: 600;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .profile {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .profile.active {
    border-color: var(--accent);
  }
  .ptop {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .plabel {
    font-weight: 600;
  }
  .pwho {
    font-size: 12.5px;
  }
  .grow {
    flex: 1;
  }
  .dirs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
  }
  .dir {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 1px 4px 1px 8px;
    border-radius: 10px;
    background: var(--bg-hover);
    font-family: var(--mono);
    font-size: 11.5px;
  }
  .dx {
    height: 16px;
    padding: 0 4px;
    border: 0;
    background: none;
    color: var(--text-faint);
  }
  .small {
    height: 24px;
    padding: 0 8px;
    font-size: 12px;
  }
  .empty {
    color: var(--text-dim);
    font-size: 12.5px;
    line-height: 1.6;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
    border: 1px solid var(--accent);
    border-radius: 8px;
  }
  .ftitle {
    font-weight: 600;
  }
  .foot {
    color: var(--text-faint);
    font-size: 11.5px;
  }
</style>
