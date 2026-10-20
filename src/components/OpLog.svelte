<script lang="ts">
  import { elapsed, type RepoTab } from "../lib/store.svelte";

  let { tab }: { tab: RepoTab } = $props();

  /** 每秒刷新一次已用时间 */
  let now = $state(Date.now());
  $effect(() => {
    if (!tab.busy) return;
    const t = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(t);
  });

  /** 从 “Receiving objects:  45% (123/456)” 里取出阶段和百分比 */
  const parsed = $derived.by(() => {
    const text = tab.progressLine.replace(/^remote:\s*/, "");
    const m = text.match(/^([^:]+):\s*(\d+)%\s*(\([^)]*\))?(.*)$/);
    if (!m) return { stage: text, percent: null as number | null, detail: "" };
    return { stage: STAGES[m[1].trim()] ?? m[1].trim(), percent: Number(m[2]), detail: [m[3], m[4]].join(" ").replace(/,?\s*done\.?$/, "").trim() };
  });

  /** git 进度阶段的中文说明 */
  const STAGES: Record<string, string> = {
    "Enumerating objects": "枚举对象",
    "Counting objects": "统计对象",
    "Compressing objects": "压缩对象",
    "Receiving objects": "接收对象",
    "Resolving deltas": "解析差异",
    "Writing objects": "上传对象",
    "Updating files": "更新文件",
    "Checking connectivity": "检查完整性",
    "Unpacking objects": "解包对象",
  };

  let scroller = $state<HTMLDivElement>();
  $effect(() => {
    void tab.log.length;
    void tab.log[tab.log.length - 1];
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
  });

  async function copy() {
    await navigator.clipboard.writeText(tab.log.map((l) => l.text).join("\n"));
  }
</script>

<div class="oplog" class:open={tab.logOpen}>
  {#if tab.busy}
    <div class="head">
      <div class="spinner"></div>
      <div class="info">
        <div class="title">
          <span>{tab.busy}</span>
          <span class="time">{now && elapsed(tab.busySince)}</span>
        </div>
        {#if parsed.stage}
          <div class="line" title={tab.progressLine}>
            {parsed.stage}{parsed.percent !== null ? ` ${parsed.percent}%` : ""}
            {#if parsed.detail}<span class="detail">{parsed.detail}</span>{/if}
          </div>
        {/if}
        {#if parsed.percent !== null}
          <div class="bar"><div style="width:{parsed.percent}%"></div></div>
        {/if}
      </div>
      <button class="small" onclick={() => (tab.logOpen = !tab.logOpen)}>{tab.logOpen ? "收起" : "日志"}</button>
      {#if tab.cancellable}<button class="small" onclick={() => tab.cancel()}>取消</button>{/if}
    </div>
  {:else}
    <div class="head">
      <div class="info"><div class="title">输出日志</div></div>
      <button class="small" onclick={copy}>复制</button>
      <button class="small" onclick={() => (tab.log = [])}>清空</button>
      <button class="small" onclick={() => (tab.logOpen = false)}>关闭</button>
    </div>
  {/if}
  {#if tab.logOpen}
    <div class="log" bind:this={scroller}>
      {#each tab.log as l, i (i)}
        <div class="l {l.kind ?? ''}">{l.text}</div>
      {:else}
        <div class="l empty">还没有输出</div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .oplog {
    position: fixed;
    left: 16px;
    bottom: 16px;
    z-index: 80;
    display: flex;
    flex-direction: column;
    width: 380px;
    max-width: calc(100vw - 32px);
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
    font-size: 13px;
  }
  .oplog.open {
    width: 640px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px 8px 14px;
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .title {
    display: flex;
    gap: 8px;
  }
  .time {
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }
  .line {
    overflow: hidden;
    color: var(--text-dim);
    font-size: 12px;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .detail {
    margin-left: 6px;
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
  .small {
    height: 24px;
    padding: 0 8px;
    font-size: 12px;
    flex-shrink: 0;
  }
  .log {
    height: 260px;
    overflow: auto;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12px;
    line-height: 1.55;
    user-select: text;
  }
  .l {
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--text-dim);
  }
  .l.cmd {
    color: var(--accent);
  }
  .l.ok {
    color: var(--text-faint);
  }
  .l.err {
    color: var(--del);
  }
  .l.empty {
    color: var(--text-faint);
  }
  .spinner {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
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
</style>
