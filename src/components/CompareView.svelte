<script lang="ts">
  import { api, type Compare } from "../lib/api";
  import { t } from "../lib/i18n";
  import { errorText, type CompareView, type RepoTab } from "../lib/store.svelte";
  import { relativeTime } from "../lib/util";
  import DiffView from "./DiffView.svelte";
  import FileList, { type FileItem } from "./FileList.svelte";

  let { tab, view, onClose }: { tab: RepoTab; view: CompareView; onClose: () => void } = $props();

  // 输入框里正在编辑的值，回车或失去焦点才生效
  // svelte-ignore state_referenced_locally
  let fromInput = $state(view.from);
  // svelte-ignore state_referenced_locally
  let toInput = $state(view.to);
  let result = $state<Compare | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let selected = $state<FileItem | null>(null);
  let showCommits = $state(false);
  let tree = $state(false);
  let toEl = $state<HTMLInputElement>();

  /** 输入框的候选：本地分支、远程分支、标签 */
  const refNames = $derived((tab.repo?.refs ?? []).filter((r) => !r.name.endsWith("/HEAD")).map((r) => r.name));

  $effect(() => {
    const { from, to, threeDot } = view;
    void tab.graphVersion;
    fromInput = from;
    toInput = to;
    if (!from.trim() || !to.trim()) {
      result = null;
      queueMicrotask(() => toEl?.focus());
      return;
    }
    let cancelled = false;
    loading = true;
    error = null;
    api
      .compare(tab.id, from, to, threeDot)
      .then((r) => {
        if (cancelled) return;
        result = r;
        // 保留之前选中的文件，没有就选第一个
        selected = items.find((f) => f.path === selected?.path) ?? items[0] ?? null;
      })
      .catch((e) => {
        if (!cancelled) {
          result = null;
          error = errorText(e);
        }
      })
      .finally(() => !cancelled && (loading = false));
    return () => (cancelled = true);
  });

  const items = $derived<FileItem[]>(
    (result?.files ?? []).map((f) => ({
      key: f.path,
      path: f.path,
      oldPath: f.old_path,
      status: f.status,
      additions: f.additions,
      deletions: f.deletions,
      binary: f.binary,
    })),
  );
  const stats = $derived(items.reduce((s, f) => ({ add: s.add + (f.additions ?? 0), del: s.del + (f.deletions ?? 0) }), { add: 0, del: 0 }));
  /** 三点对比时文件 diff 从共同祖先算起 */
  const diffFrom = $derived(result ? (view.threeDot && result.base ? result.base : result.from) : "");

  function apply() {
    if (fromInput.trim() !== view.from || toInput.trim() !== view.to) tab.compareView = { ...view, from: fromInput.trim(), to: toInput.trim() };
  }

  function swap() {
    tab.compareView = { ...view, from: view.to, to: view.from };
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.isComposing) apply();
  }
</script>

<div class="cv">
  <div class="bar">
    <span class="label">{t("对比")}</span>
    <input class="rev" bind:value={fromInput} list="cmp-refs" onkeydown={onKey} onblur={apply} placeholder={t("分支、标签或提交号")} spellcheck="false" />
    <button class="swap" onclick={swap} title={t("交换两边")}>⇄</button>
    <input
      class="rev"
      bind:this={toEl}
      bind:value={toInput}
      list="cmp-refs"
      onkeydown={onKey}
      onblur={apply}
      placeholder={t("分支、标签或提交号")}
      spellcheck="false"
    />
    <datalist id="cmp-refs">
      {#each refNames as n (n)}<option value={n}></option>{/each}
    </datalist>
    <select
      value={view.threeDot ? "3" : "2"}
      onchange={(e) => (tab.compareView = { ...view, threeDot: e.currentTarget.value === "3" })}
      title={t("“新增的改动”和 GitHub / GitLab 的 Compare 一样，只看右边相对共同祖先多出来的改动")}
    >
      <option value="3">{t("只看右边新增的改动")}</option>
      <option value="2">{t("直接对比两端")}</option>
    </select>
    <span class="grow"></span>
    <button onclick={onClose} title={t("关闭 (Esc)")}>✕</button>
  </div>

  {#if error}
    <div class="msg error">{error}</div>
  {:else if !view.from.trim() || !view.to.trim()}
    <div class="msg">{t("输入要对比的两个分支、标签或提交号，回车开始对比")}</div>
  {:else if loading && !result}
    <div class="msg">{t("加载中…")}</div>
  {:else if result}
    <div class="body">
      <div class="side">
        <div class="summary">
          <button class="commits-toggle" onclick={() => (showCommits = !showCommits)}>
            {showCommits ? "▾" : "▸"}
            {t("{0} 个提交只在 {1} 上", result.ahead, view.to)}
          </button>
          {#if result.behind}<div class="behind">{t("{0} 落后 {1} {2} 个提交", view.to, view.from, result.behind)}</div>{/if}
          {#if showCommits}
            <div class="commits">
              {#each result.commits as c (c.oid)}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div class="commit" ondblclick={() => tab.jumpToOid(c.oid)} title={t("双击在提交图中定位")}>
                  <span class="csum">{c.summary}</span>
                  <span class="cmeta">{c.author} · {relativeTime(c.time)} · <span class="mono">{c.oid.slice(0, 7)}</span></span>
                </div>
              {/each}
              {#if result.ahead > result.commits.length}<div class="more">{t("只显示前 {0} 个", result.commits.length)}</div>{/if}
            </div>
          {/if}
          <div class="fstat">
            <span>{t("{0} 个文件改动", items.length)}</span>
            <span class="add">+{stats.add}</span>
            <span class="del">-{stats.del}</span>
            <span class="grow"></span>
            <button class="small" onclick={() => (tree = !tree)}>{tree ? t("列表") : t("目录树")}</button>
          </div>
        </div>
        <div class="files">
          {#if items.length}
            <FileList files={items} {tree} active={selected?.key ?? null} onOpen={(f) => (selected = f)} />
          {:else}
            <div class="msg">{t("两边的文件内容完全相同")}</div>
          {/if}
        </div>
      </div>
      <div class="diff">
        {#if selected}
          {#key `${diffFrom}:${result.to}:${selected.path}`}
            <DiffView
              {tab}
              embedded
              target={{ source: "compare", from: diffFrom, to: result.to, path: selected.path, oldPath: selected.oldPath }}
              {onClose}
            />
          {/key}
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .cv {
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
  .label {
    font-weight: 600;
  }
  .rev {
    width: 200px;
    height: 28px;
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .swap {
    width: 28px;
    height: 28px;
    padding: 0;
  }
  select {
    height: 28px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
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
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 340px 1fr;
  }
  .side {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border);
  }
  .summary {
    flex-shrink: 0;
    border-bottom: 1px solid var(--border);
  }
  .commits-toggle {
    width: 100%;
    height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: 0;
    background: none;
    text-align: left;
    font-weight: 600;
  }
  .behind {
    padding: 0 12px 6px 26px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .commits {
    max-height: 40vh;
    overflow: auto;
    border-top: 1px solid var(--border);
  }
  .commit {
    display: flex;
    flex-direction: column;
    padding: 6px 12px;
    cursor: default;
  }
  .commit:hover {
    background: var(--bg-hover);
  }
  .csum {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 12.5px;
  }
  .cmeta {
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .mono {
    font-family: var(--mono);
  }
  .more {
    padding: 6px 12px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .fstat {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-top: 1px solid var(--border);
    font-size: 12.5px;
  }
  .add {
    color: var(--add);
  }
  .del {
    color: var(--del);
  }
  .small {
    height: 24px;
    padding: 0 8px;
    font-size: 12px;
  }
  .files {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .diff {
    min-width: 0;
    min-height: 0;
  }
</style>
