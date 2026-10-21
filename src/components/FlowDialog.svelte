<script lang="ts">
  import { t } from "../lib/i18n";
  import { untrack } from "svelte";
  import type { FlowConfig, FlowKind } from "../lib/api";
  import { app } from "../lib/store.svelte";

  const spec = $derived(app.flowDialog!);
  const tab = $derived(spec.tab);
  const cfg = $derived(tab.flow);

  const KINDS: { kind: FlowKind; label: string; desc: string }[] = [
    { kind: "feature", label: "Feature", desc: t("新功能，从开发分支开始，完成后合并回开发分支") },
    { kind: "bugfix", label: "Bugfix", desc: t("开发中的问题修复，从开发分支开始，完成后合并回开发分支") },
    { kind: "release", label: "Release", desc: t("准备发布新版本，完成后合并到生产分支并打版本标签，再合并回开发分支") },
    { kind: "hotfix", label: "Hotfix", desc: t("线上紧急修复，从生产分支开始，完成后合并到生产分支并打标签，再合并回开发分支") },
  ];

  // ---------- 表单状态 ----------
  let init = $state<FlowConfig>({
    master: "main",
    develop: "develop",
    feature: "feature/",
    bugfix: "bugfix/",
    release: "release/",
    hotfix: "hotfix/",
    version_tag: "",
    initialized: false,
  });
  let kind = $state<FlowKind>("feature");
  let name = $state("");
  let base = $state("");
  let keepBranch = $state(false);
  let push = $state(false);
  let tagMessage = $state("");
  let working = $state(false);
  let nameInput = $state<HTMLInputElement>();

  // 打开时根据模式填好默认值（只跟随对话框本身，后台刷新不会清掉正在填的内容）
  $effect(() => {
    const s = spec;
    untrack(() => fill(s));
  });

  function fill(s: typeof spec) {
    if (cfg) init = { ...cfg };
    kind = s.kind ?? "feature";
    name = s.name ?? "";
    keepBranch = false;
    push = false;
    tagMessage = "";
    if (s.mode === "start") resetStart(kind);
    queueMicrotask(() => nameInput?.focus());
  }

  function resetStart(k: FlowKind) {
    kind = k;
    base = k === "hotfix" ? (cfg?.master ?? "") : (cfg?.develop ?? "");
    name = k === "release" || k === "hotfix" ? tab.suggestVersion(k) : "";
    queueMicrotask(() => {
      nameInput?.focus();
      nameInput?.select();
    });
  }

  const locals = $derived((tab.repo?.refs ?? []).filter((r) => r.kind === "local").map((r) => r.name).sort());
  const isVersion = $derived(kind === "release" || kind === "hotfix");
  const prefix = $derived(cfg ? cfg[kind] : "");
  const branch = $derived(`${prefix}${name.trim()}`);
  const tag = $derived(`${cfg?.version_tag ?? ""}${name.trim()}`);
  const dirty = $derived(tab.changeCount > 0);

  /** 完成时会执行的步骤，显示给用户确认 */
  const finishSteps = $derived.by(() => {
    if (!cfg) return [];
    const steps = isVersion
      ? [t("把 {0} 合并到 {1}", branch, cfg.master), t("在 {0} 上打标签 {1}", cfg.master, tag), t("把标签 {0} 合并回 {1}", tag, cfg.develop)]
      : [t("把 {0} 合并到 {1}", branch, cfg.develop)];
    steps.push(t("切换到 {0}", cfg.develop));
    if (!keepBranch) steps.push(t("删除本地分支 {0}", branch));
    if (push) steps.push(isVersion ? t("推送 {0}、{1} 和标签 {2}", cfg.master, cfg.develop, tag) : t("推送 {0}", cfg.develop));
    return steps;
  });

  function close() {
    app.flowDialog = null;
  }

  async function submit() {
    if (working) return;
    working = true;
    let ok = false;
    if (spec.mode === "init") {
      ok = await tab.run({ op: "flow_init", config: { ...init, initialized: true } }, t("正在初始化 Git Flow…"));
    } else if (spec.mode === "start") {
      if (!name.trim()) {
        working = false;
        return nameInput?.focus();
      }
      ok = await tab.run({ op: "flow_start", kind, name: name.trim(), base: base || null }, t("正在创建 {0}…", branch));
    } else {
      ok = await tab.run(
        {
          op: "flow_finish",
          kind,
          name: name.trim(),
          options: { keep_branch: keepBranch, tag_message: tagMessage.trim() || null, push },
        },
        t("正在完成 {0}…", branch),
      );
    }
    working = false;
    if (ok) close();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
    if (e.key === "Enter" && !e.isComposing && !(e.target instanceof HTMLTextAreaElement)) {
      e.preventDefault();
      submit();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={onKey}>
  <div class="dialog" role="dialog" aria-modal="true">
    {#if spec.mode === "init"}
      <div class="title">{cfg?.initialized ? t("Git Flow 设置") : t("初始化 Git Flow")}</div>
      <div class="hint">{t("设置保存在仓库的 git config 里，和命令行的 git flow、GitKraken、SourceTree 通用。")}</div>
      <div class="grid">
        <label for="f-master">{t("生产分支")}</label>
        <select id="f-master" bind:value={init.master}>
          {#each locals.includes(init.master) ? locals : [init.master, ...locals] as b (b)}
            <option value={b}>{b}</option>
          {/each}
        </select>
        <label for="f-develop">{t("开发分支")}</label>
        <input id="f-develop" bind:value={init.develop} spellcheck="false" />
        <label for="f-feature">{t("Feature 前缀")}</label>
        <input id="f-feature" bind:value={init.feature} spellcheck="false" />
        <label for="f-bugfix">{t("Bugfix 前缀")}</label>
        <input id="f-bugfix" bind:value={init.bugfix} spellcheck="false" />
        <label for="f-release">{t("Release 前缀")}</label>
        <input id="f-release" bind:value={init.release} spellcheck="false" />
        <label for="f-hotfix">{t("Hotfix 前缀")}</label>
        <input id="f-hotfix" bind:value={init.hotfix} spellcheck="false" />
        <label for="f-tag">{t("版本标签前缀")}</label>
        <input id="f-tag" bind:value={init.version_tag} placeholder={t("例如 v，可以留空")} spellcheck="false" />
      </div>
      {#if !locals.includes(init.develop)}
        <div class="note">{t("开发分支 {0} 还不存在，会从 {1} 创建。", init.develop, init.master)}</div>
      {/if}
    {:else if spec.mode === "start"}
      <div class="title">{t("开始新的 Git Flow 分支")}</div>
      <div class="kinds">
        {#each KINDS as k (k.kind)}
          <button class="kind" class:on={kind === k.kind} onclick={() => resetStart(k.kind)}>{k.label}</button>
        {/each}
      </div>
      <div class="hint">{KINDS.find((k) => k.kind === kind)?.desc}</div>
      <div class="grid">
        <label for="f-name">{isVersion ? t("版本号") : t("名称")}</label>
        <div class="prefixed">
          <span class="pfx">{prefix}</span>
          <input
            id="f-name"
            bind:this={nameInput}
            bind:value={name}
            placeholder={isVersion ? t("例如 1.2.0") : t("例如 user-login")}
            spellcheck="false"
          />
        </div>
        <label for="f-base">{t("基于")}</label>
        <select id="f-base" bind:value={base}>
          {#each locals.includes(base) ? locals : [base, ...locals] as b (b)}
            <option value={b}>{b}</option>
          {/each}
        </select>
      </div>
      {#if isVersion && name.trim()}
        <div class="note">{t("完成时会打标签")} <b>{tag}</b></div>
      {/if}
      {#if dirty}
        <div class="note">{t("当前未提交的更改会带到新分支上。")}</div>
      {/if}
    {:else}
      <div class="title">{t("完成 {0}", branch)}</div>
      <ol class="steps">
        {#each finishSteps as s, i (i)}<li>{s}</li>{/each}
      </ol>
      {#if isVersion}
        <label class="field">
          <span>{t("标签说明")}</span>
          <textarea bind:value={tagMessage} rows="3" placeholder={t("默认为 “{0} {1}”", kind === "release" ? "Release" : "Hotfix", name)}></textarea>
        </label>
      {/if}
      <label class="check"><input type="checkbox" bind:checked={keepBranch} /> {t("完成后保留 {0} 分支", branch)}</label>
      <label class="check"><input type="checkbox" bind:checked={push} /> {t("完成后推送到 origin")}</label>
      {#if dirty}
        <div class="warn">{t("有未提交的更改，需要先提交或 stash 才能完成。")}</div>
      {/if}
      <div class="hint">{t("如果合并时出现冲突，解决冲突并提交后再点一次“完成”，已经做完的步骤会自动跳过。")}</div>
    {/if}

    <div class="buttons">
      <button onclick={close}>{t("取消")}</button>
      <button class="primary" disabled={working || (spec.mode === "finish" && dirty)} onclick={submit}>
        {spec.mode === "init" ? (cfg?.initialized ? t("保存") : t("初始化")) : spec.mode === "start" ? t("开始") : t("完成")}
      </button>
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
    width: 500px;
    max-width: calc(100vw - 32px);
    max-height: 80vh;
    overflow: auto;
    padding: 18px 20px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
  }
  .title {
    font-size: 15px;
    font-weight: 600;
  }
  .hint {
    color: var(--text-dim);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .grid {
    display: grid;
    grid-template-columns: 96px 1fr;
    align-items: center;
    gap: 8px 12px;
    font-size: 13px;
  }
  .grid label {
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
  .prefixed {
    display: flex;
    align-items: center;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--input-bg);
  }
  .prefixed:focus-within {
    border-color: var(--accent);
  }
  .pfx {
    padding-left: 9px;
    color: var(--text-faint);
    font-family: var(--mono);
    font-size: 12.5px;
    white-space: nowrap;
  }
  .prefixed input {
    flex: 1;
    min-width: 0;
    border: 0;
    padding-left: 2px;
    background: none;
  }
  .kinds {
    display: flex;
    gap: 6px;
  }
  .kind {
    flex: 1;
  }
  .kind.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .note {
    color: var(--text-dim);
    font-size: 12.5px;
  }
  .warn {
    padding: 8px 10px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--warn) 18%, transparent);
    font-size: 12.5px;
  }
  .steps {
    margin: 0;
    padding-left: 22px;
    font-size: 13px;
    line-height: 1.8;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
    color: var(--text-dim);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
</style>
