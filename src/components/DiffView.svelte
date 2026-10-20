<script lang="ts">
  import type { DiffLine, FileDiff } from "../lib/api";
  import { confirm, errorText, type DiffTarget, type RepoTab } from "../lib/store.svelte";

  let {
    tab,
    target,
    onClose,
    embedded = false,
  }: {
    tab: RepoTab;
    target: DiffTarget;
    onClose: () => void;
    /** 嵌在文件历史里：不显示历史 / Blame / 关闭按钮 */
    embedded?: boolean;
  } = $props();

  const LINE_H = 20;
  let diff = $state<FileDiff | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let split = $state(loadSplitPref());
  let scroller = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewH = $state(600);

  function loadSplitPref() {
    try {
      return localStorage.getItem("gittree.diffSplit") === "1";
    } catch {
      return false;
    }
  }
  function toggleSplit() {
    split = !split;
    try {
      localStorage.setItem("gittree.diffSplit", split ? "1" : "0");
    } catch {
      // 忽略
    }
  }

  // 目标文件变化或仓库刷新后重新加载
  $effect(() => {
    const t = target;
    void tab.graphVersion;
    void tab.status;
    let cancelled = false;
    loading = true;
    tab
      .loadDiff(t)
      .then((d) => {
        if (cancelled) return;
        diff = d;
        error = null;
      })
      .catch((e) => {
        if (!cancelled) error = errorText(e);
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => (cancelled = true);
  });

  type Item =
    | { t: "hunk"; h: number; header: string }
    | { t: "line"; line: DiffLine }
    | { t: "pair"; left: DiffLine | null; right: DiffLine | null };

  const items = $derived.by(() => {
    const out: Item[] = [];
    if (!diff) return out;
    diff.hunks.forEach((hunk, h) => {
      out.push({ t: "hunk", h, header: hunk.header });
      if (!split) {
        for (const line of hunk.lines) out.push({ t: "line", line });
        return;
      }
      // 并排视图：把相邻的删除行和新增行配对
      const lines = hunk.lines;
      let i = 0;
      while (i < lines.length) {
        if (lines[i].kind === " ") {
          out.push({ t: "pair", left: lines[i], right: lines[i] });
          i++;
          continue;
        }
        const dels: DiffLine[] = [];
        const adds: DiffLine[] = [];
        while (i < lines.length && lines[i].kind === "-") dels.push(lines[i++]);
        while (i < lines.length && lines[i].kind === "+") adds.push(lines[i++]);
        for (let k = 0; k < Math.max(dels.length, adds.length); k++) {
          out.push({ t: "pair", left: dels[k] ?? null, right: adds[k] ?? null });
        }
      }
    });
    return out;
  });

  const start = $derived(Math.max(0, Math.floor(scrollTop / LINE_H) - 20));
  const end = $derived(Math.min(items.length, Math.ceil((scrollTop + viewH) / LINE_H) + 20));
  const stats = $derived.by(() => {
    let add = 0;
    let del = 0;
    for (const h of diff?.hunks ?? [])
      for (const l of h.lines) {
        if (l.kind === "+") add++;
        else if (l.kind === "-") del++;
      }
    return { add, del };
  });

  const canStageHunk = $derived(target.source === "workdir");

  async function hunkAction(header: string, action: "stage" | "unstage" | "discard") {
    if (action === "discard") {
      if (!(await confirm("丢弃代码块", "确定丢弃这个代码块的改动吗？此操作无法撤销。", "丢弃", true))) return;
      await tab.run({ op: "discard_hunk", path: target.path, header });
    } else {
      await tab.run({ op: "stage_hunk", path: target.path, header, staged: action === "unstage" });
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="diff">
  <div class="bar">
    <div class="title">
      {#if target.oldPath && target.oldPath !== target.path}
        <span class="old">{target.oldPath}</span> →
      {/if}
      <span class="path">{target.path}</span>
      {#if target.source === "workdir"}
        <span class="tag">{target.staged ? "已暂存" : "未暂存"}</span>
      {:else if target.oid}
        <span class="tag mono">{target.oid.slice(0, 7)}</span>
      {/if}
      <span class="add">+{stats.add}</span>
      <span class="del">-{stats.del}</span>
    </div>
    <div class="actions">
      {#if !embedded}
        <button onclick={() => tab.openFile(target.path, "history")} title="查看修改过这个文件的所有提交">历史</button>
        <button
          onclick={() => tab.openFile(target.path, "blame", target.source === "commit" ? target.oid! : null)}
          title="逐行查看是谁在哪次提交里改的">Blame</button
        >
      {/if}
      <button onclick={toggleSplit}>{split ? "上下对比" : "左右对比"}</button>
      {#if !embedded}<button onclick={onClose} title="关闭 (Esc)">✕</button>{/if}
    </div>
  </div>
  {#if error}
    <div class="msg error">{error}</div>
  {:else if loading && !diff}
    <div class="msg">加载中…</div>
  {:else if !diff}
    <div class="msg">没有改动</div>
  {:else if diff.binary}
    <div class="msg">二进制文件，无法显示差异</div>
  {:else if diff.hunks.length === 0}
    <div class="msg">没有文本改动（可能只是文件权限变化）</div>
  {:else}
    <div
      class="scroller"
      bind:this={scroller}
      bind:clientHeight={viewH}
      onscroll={() => (scrollTop = scroller!.scrollTop)}
    >
      <div class="spacer" class:split style="height:{items.length * LINE_H + (diff.truncated ? 40 : 0)}px">
        <div class="lines" style="transform: translateY({start * LINE_H}px)">
          {#each items.slice(start, end) as it, k (start + k)}
            {#if it.t === "hunk"}
              <div class="hunk" style="height:{LINE_H}px">
                <span class="hh">{it.header}</span>
                {#if canStageHunk}
                  <span class="hunk-actions">
                    {#if target.staged}
                      <button onclick={() => hunkAction(it.header, "unstage")}>取消暂存代码块</button>
                    {:else}
                      <button onclick={() => hunkAction(it.header, "stage")}>暂存代码块</button>
                      <button class="danger" onclick={() => hunkAction(it.header, "discard")}>丢弃</button>
                    {/if}
                  </span>
                {/if}
              </div>
            {:else if it.t === "line"}
              <div class="line k{it.line.kind === '+' ? 'add' : it.line.kind === '-' ? 'del' : 'ctx'}" style="height:{LINE_H}px">
                <span class="no">{it.line.old ?? ""}</span>
                <span class="no">{it.line.new ?? ""}</span>
                <span class="sign">{it.line.kind}</span>
                <span class="code">{it.line.text}</span>
              </div>
            {:else}
              <div class="line pair" style="height:{LINE_H}px">
                <div class="half {it.left ? (it.left.kind === '-' ? 'kdel' : 'kctx') : 'kempty'}">
                  <span class="no">{it.left?.old ?? ""}</span>
                  <span class="code">{it.left?.text ?? ""}</span>
                </div>
                <div class="half {it.right ? (it.right.kind === '+' ? 'kadd' : 'kctx') : 'kempty'}">
                  <span class="no">{it.right?.new ?? ""}</span>
                  <span class="code">{it.right?.text ?? ""}</span>
                </div>
              </div>
            {/if}
          {/each}
        </div>
        {#if diff.truncated && end >= items.length}
          <div class="truncated" style="top:{items.length * LINE_H}px">文件改动太多，只显示了前 20000 行</div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .diff {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-graph);
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    height: 38px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
  }
  .path {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .old {
    color: var(--text-dim);
  }
  .tag {
    padding: 1px 6px;
    border-radius: 3px;
    background: var(--bg-hover);
    color: var(--text-dim);
    font-size: 12px;
  }
  .mono {
    font-family: var(--mono);
  }
  .add {
    color: var(--add);
  }
  .del {
    color: var(--del);
  }
  .actions {
    display: flex;
    gap: 6px;
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
  .scroller {
    flex: 1;
    overflow: auto;
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .spacer {
    position: relative;
    min-width: 100%;
    width: max-content;
  }
  .lines {
    will-change: transform;
  }
  .line,
  .hunk {
    display: flex;
    align-items: center;
    white-space: pre;
    line-height: 20px;
  }
  .hunk {
    position: sticky;
    left: 0;
    gap: 12px;
    padding-left: 8px;
    background: var(--bg-hunk);
    color: var(--text-dim);
  }
  .hunk-actions {
    display: flex;
    gap: 6px;
    font-family: var(--sans);
  }
  .hunk-actions button {
    height: 18px;
    padding: 0 8px;
    font-size: 11.5px;
  }
  .no {
    display: inline-block;
    width: 48px;
    padding-right: 8px;
    text-align: right;
    color: var(--text-faint);
    flex-shrink: 0;
    user-select: none;
  }
  .sign {
    width: 16px;
    flex-shrink: 0;
    color: var(--text-faint);
    user-select: none;
  }
  .code {
    padding-right: 24px;
  }
  .kadd {
    background: var(--bg-add);
  }
  .kdel {
    background: var(--bg-del);
  }
  .kempty {
    background: var(--bg-empty);
  }
  .pair {
    width: 100%;
  }
  .spacer.split {
    width: 100%;
  }
  .half {
    display: flex;
    flex: 1 1 0;
    min-width: 0;
    height: 100%;
    overflow: hidden;
  }
  .half + .half {
    border-left: 1px solid var(--border);
  }
  .truncated {
    position: absolute;
    left: 0;
    right: 0;
    padding: 10px;
    text-align: center;
    color: var(--warn);
    font-family: var(--sans);
  }
</style>
