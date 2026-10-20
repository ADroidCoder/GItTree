<script lang="ts">
  import { api, type Blame, type HistoryEntry } from "../lib/api";
  import { errorText, type FileView, type RepoTab } from "../lib/store.svelte";
  import { avatarColor, formatDate, initials, relativeTime } from "../lib/util";
  import DiffView from "./DiffView.svelte";

  let { tab, view, onClose }: { tab: RepoTab; view: FileView; onClose: () => void } = $props();

  // ---------- 文件历史 ----------
  let history = $state<HistoryEntry[] | null>(null);
  let historyError = $state<string | null>(null);
  let selected = $state<HistoryEntry | null>(null);

  $effect(() => {
    if (view.mode !== "history") return;
    const p = view.path;
    let cancelled = false;
    history = null;
    historyError = null;
    selected = null;
    api
      .fileHistory(tab.id, p)
      .then((h) => {
        if (cancelled) return;
        history = h;
        selected = h[0] ?? null;
      })
      .catch((e) => !cancelled && (historyError = errorText(e)));
    return () => (cancelled = true);
  });

  // ---------- Blame ----------
  const LINE_H = 20;
  let blame = $state<Blame | null>(null);
  let blameError = $state<string | null>(null);
  let scroller = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewH = $state(600);

  $effect(() => {
    if (view.mode !== "blame") return;
    const { path, rev } = view;
    void tab.status;
    let cancelled = false;
    blameError = null;
    api
      .blame(tab.id, path, rev)
      .then((b) => !cancelled && (blame = b))
      .catch((e) => {
        if (!cancelled) {
          blame = null;
          blameError = errorText(e);
        }
      });
    return () => (cancelled = true);
  });

  const start = $derived(Math.max(0, Math.floor(scrollTop / LINE_H) - 20));
  const end = $derived(Math.min(blame?.lines.length ?? 0, Math.ceil((scrollTop + viewH) / LINE_H) + 20));

  /** 按提交时间给左边的色条上色：越新越亮 */
  const age = $derived.by(() => {
    const times = (blame?.commits ?? []).filter((c) => !c.uncommitted).map((c) => c.time);
    const min = Math.min(...times);
    const max = Math.max(...times);
    return (t: number) => (max > min ? (t - min) / (max - min) : 1);
  });

  /** 这一行是不是某个提交连续区域的第一行 */
  const firstOfBlock = (i: number) => i === 0 || blame!.line_commit[i] !== blame!.line_commit[i - 1];

  function gotoCommit(oid: string) {
    onClose();
    tab.jumpToOid(oid);
  }

  function setMode(mode: FileView["mode"]) {
    tab.fileView = { ...view, mode };
  }
</script>

<div class="fv">
  <div class="bar">
    <div class="title">
      <span class="path">{view.path}</span>
      {#if view.mode === "blame"}
        <span class="tag mono">{view.rev ? view.rev.slice(0, 7) : "工作区"}</span>
      {/if}
    </div>
    <div class="tabs">
      <button class:on={view.mode === "history"} onclick={() => setMode("history")}>文件历史</button>
      <button class:on={view.mode === "blame"} onclick={() => setMode("blame")}>Blame</button>
    </div>
    <button onclick={onClose} title="关闭 (Esc)">✕</button>
  </div>

  {#if view.mode === "history"}
    {#if historyError}
      <div class="msg error">{historyError}</div>
    {:else if !history}
      <div class="msg">加载中…</div>
    {:else if history.length === 0}
      <div class="msg">这个文件还没有提交记录</div>
    {:else}
      <div class="hist">
        <div class="hlist">
          <div class="hcount">{history.length} 个提交修改过这个文件{history.length >= 5000 ? "（只显示最近 5000 个）" : ""}</div>
          {#each history as h (h.oid)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="hitem" class:sel={selected?.oid === h.oid} onclick={() => (selected = h)} ondblclick={() => gotoCommit(h.oid)} title="双击在提交图中定位">
              <span class="avatar" style="background:{avatarColor(h.email || h.author)}">{initials(h.author)}</span>
              <div class="hmain">
                <div class="hsum">{h.summary}</div>
                <div class="hmeta">
                  <span>{h.author}</span>
                  <span title={formatDate(h.time)}>{relativeTime(h.time)}</span>
                  <span class="mono">{h.oid.slice(0, 7)}</span>
                  {#if h.old_path}<span class="ren" title="{h.old_path} → {h.path}">重命名</span>{/if}
                </div>
              </div>
            </div>
          {/each}
        </div>
        <div class="hdiff">
          {#if selected}
            {@const s = selected}
            <div class="hbar">
              <span class="hsum2">{s.summary}</span>
              <button class="small" onclick={() => tab.openFile(s.path, "blame", s.oid)}>Blame 这个版本</button>
              <button class="small" onclick={() => gotoCommit(s.oid)}>在图中定位</button>
            </div>
            <div class="hdiff-body">
              {#key s.oid}
                <DiffView {tab} embedded target={{ source: "commit", oid: s.oid, path: s.path, oldPath: s.old_path }} {onClose} />
              {/key}
            </div>
          {/if}
        </div>
      </div>
    {/if}
  {:else if blameError}
    <div class="msg error">{blameError}</div>
  {:else if !blame}
    <div class="msg">加载中…</div>
  {:else}
    <div class="scroller" bind:this={scroller} bind:clientHeight={viewH} onscroll={() => (scrollTop = scroller!.scrollTop)}>
      <div class="spacer" style="height:{blame.lines.length * LINE_H}px">
        <div class="lines" style="transform: translateY({start * LINE_H}px)">
          {#each blame.lines.slice(start, end) as text, k (start + k)}
            {@const i = start + k}
            {@const c = blame.commits[blame.line_commit[i]]}
            <div class="bline" class:first={firstOfBlock(i)} style="height:{LINE_H}px">
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="gutter"
                class:uncommitted={c.uncommitted}
                style="--age:{c.uncommitted ? 1 : age(c.time)}"
                title={c.uncommitted ? "还没有提交的改动" : `${c.summary}\n${c.author} · ${formatDate(c.time)} · ${c.oid.slice(0, 7)}\n点击在提交图中定位`}
                onclick={() => !c.uncommitted && gotoCommit(c.oid)}
              >
                {#if firstOfBlock(i)}
                  {#if c.uncommitted}
                    <span class="who">未提交</span>
                  {:else}
                    <span class="who">{c.author}</span>
                    <span class="when">{relativeTime(c.time)}</span>
                    <span class="bsum">{c.summary}</span>
                  {/if}
                {/if}
              </div>
              <span class="no">{i + 1}</span>
              <span class="code">{text}</span>
            </div>
          {/each}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .fv {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-graph);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 38px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .title {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }
  .path {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tag {
    padding: 1px 6px;
    border-radius: 3px;
    background: var(--bg-hover);
    font-size: 11.5px;
  }
  .mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  .tabs {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 5px;
    overflow: hidden;
  }
  .tabs button {
    height: 26px;
    border: 0;
    border-radius: 0;
    font-size: 12px;
  }
  .tabs button.on {
    background: var(--accent);
    color: #fff;
  }
  .msg {
    padding: 40px;
    text-align: center;
    color: var(--text-dim);
  }
  .msg.error {
    color: var(--del);
  }
  .hist {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 320px 1fr;
  }
  .hlist {
    overflow: auto;
    border-right: 1px solid var(--border);
  }
  .hcount {
    padding: 8px 12px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .hitem {
    display: flex;
    gap: 8px;
    padding: 7px 12px;
    cursor: default;
  }
  .hitem:hover {
    background: var(--bg-hover);
  }
  .hitem.sel {
    background: var(--bg-selected);
  }
  .avatar {
    flex-shrink: 0;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    color: #fff;
    font-size: 11px;
    line-height: 24px;
    text-align: center;
  }
  .hmain {
    min-width: 0;
    flex: 1;
  }
  .hsum {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 13px;
  }
  .hmeta {
    display: flex;
    gap: 8px;
    color: var(--text-faint);
    font-size: 11.5px;
    white-space: nowrap;
  }
  .ren {
    color: var(--accent);
  }
  .hdiff {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .hbar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .hsum2 {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-weight: 600;
  }
  .hdiff-body {
    flex: 1;
    min-height: 0;
  }
  .small {
    height: 24px;
    padding: 0 8px;
    font-size: 12px;
  }
  .scroller {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .spacer {
    position: relative;
  }
  .lines {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
  }
  .bline {
    display: flex;
    align-items: center;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12.5px;
    white-space: pre;
  }
  .bline.first {
    box-shadow: inset 0 1px 0 var(--border);
  }
  .gutter {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-shrink: 0;
    width: 340px;
    height: 100%;
    padding: 0 10px 0 12px;
    border-left: 4px solid color-mix(in srgb, var(--accent) calc(var(--age) * 100%), var(--border));
    background: var(--bg-side);
    color: var(--text-dim);
    font-family: inherit;
    font-size: 12px;
    overflow: hidden;
    cursor: pointer;
  }
  .gutter.uncommitted {
    border-left-color: var(--warn);
    cursor: default;
  }
  .gutter:hover {
    background: var(--bg-hover);
  }
  .who {
    flex-shrink: 0;
    max-width: 90px;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
  }
  .when {
    flex-shrink: 0;
    color: var(--text-faint);
  }
  .bsum {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .no {
    flex-shrink: 0;
    width: 52px;
    padding-right: 10px;
    color: var(--text-faint);
    text-align: right;
  }
  .code {
    flex: 1;
  }
</style>
