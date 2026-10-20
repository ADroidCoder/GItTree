<script lang="ts">
  import { api, type PlanCommit, type RebaseStep } from "../lib/api";
  import { app, errorText } from "../lib/store.svelte";
  import { relativeTime } from "../lib/util";

  // svelte-ignore state_referenced_locally
  const { tab, from } = app.rebaseDialog!;

  type Action = RebaseStep["action"];
  interface Item {
    c: PlanCommit;
    action: Action;
    /** 用户改过的提交说明（对保留的提交有效，是合并后整组的说明） */
    edited: string | null;
  }

  /** 和提交图一样，新的在上面 */
  let items = $state<Item[]>([]);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let editing = $state<string | null>(null);
  let running = $state(false);
  /** 原来的顺序，用来判断有没有改动和“全部还原” */
  let original: PlanCommit[] = [];

  api
    .rebasePlan(tab.id, from)
    .then((list) => {
      original = list.reverse();
      items = original.map((c) => ({ c, action: "pick", edited: null }));
    })
    .catch((e) => (error = errorText(e)))
    .finally(() => (loading = false));

  /** 每个保留的提交会并进哪些提交：按执行顺序（旧 → 新），合并的提交并进它前面最近的保留提交 */
  const groups = $derived.by(() => {
    const out = new Map<string, Item[]>();
    let head: Item | null = null;
    for (const it of [...items].reverse()) {
      if (it.action === "pick") {
        head = it;
        out.set(it.c.oid, []);
      } else if (it.action === "squash" && head) {
        out.get(head.c.oid)!.push(it);
      }
    }
    return out;
  });

  /** 保留的提交最终的说明：改过就用改过的，否则把合并进来的提交说明接在后面 */
  function messageOf(it: Item): string {
    if (it.edited !== null) return it.edited;
    const sq = groups.get(it.c.oid) ?? [];
    return [it.c.message, ...sq.map((s) => s.c.message)].join("\n\n");
  }

  /** 合并到哪个提交（执行顺序里前面最近的保留提交） */
  function squashTarget(i: number): Item | null {
    for (let k = i + 1; k < items.length; k++) {
      if (items[k].action === "pick") return items[k];
    }
    return null;
  }

  const merges = $derived(items.filter((it) => it.c.parents > 1).length);
  const problem = $derived.by(() => {
    if (merges) return `范围内有 ${merges} 个合并提交，交互式变基暂不支持合并提交，请选择更近的起点`;
    const kept = items.filter((it) => it.action !== "drop");
    if (!kept.length) return "至少要保留一个提交";
    if (kept[kept.length - 1].action === "squash") return "最下面（最早）的提交不能合并到下面，它下面没有提交了";
    return null;
  });
  const changed = $derived(
    items.some((it, i) => it.action !== "pick" || it.edited !== null || it.c.oid !== original[i]?.oid),
  );

  function move(i: number, d: number) {
    const j = i + d;
    if (j < 0 || j >= items.length) return;
    const next = [...items];
    [next[i], next[j]] = [next[j], next[i]];
    items = next;
  }

  // ---------- 拖动排序 ----------
  let dragIdx = $state<number | null>(null);
  let overIdx = $state<number | null>(null);
  function drop(i: number) {
    if (dragIdx === null || dragIdx === i) return;
    const next = [...items];
    const [it] = next.splice(dragIdx, 1);
    next.splice(i, 0, it);
    items = next;
    dragIdx = overIdx = null;
  }

  function setAction(it: Item, a: Action) {
    it.action = a;
    if (a !== "pick") it.edited = null;
  }

  function reset() {
    items = original.map((c) => ({ c, action: "pick", edited: null }));
  }

  async function start() {
    if (problem || running) return;
    // 后端按执行顺序（旧 → 新）
    const steps: RebaseStep[] = [...items].reverse().map((it) => {
      if (it.action !== "pick") return { oid: it.c.oid, action: it.action, message: null };
      const msg = messageOf(it);
      return { oid: it.c.oid, action: "pick", message: msg.trim() === it.c.message.trim() ? null : msg };
    });
    running = true;
    app.rebaseDialog = null;
    await tab.run({ op: "rebase_interactive", from, steps }, "正在变基…");
  }

  function close() {
    if (!running) app.rebaseDialog = null;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !editing && close()} />

<div class="overlay">
  <div class="dialog" role="dialog" aria-modal="true">
    <div class="head">
      <div class="title">交互式变基 · {tab.repo?.head.branch ?? "HEAD"}</div>
      <div class="sub">
        和提交图一样，新的提交在上面。拖动 ⋮⋮ 调整顺序；“合并到下面”会把这个提交并进下面最近的保留提交；点提交说明可以修改。
        做错了可以用工具栏的「撤销」退回。
      </div>
    </div>

    {#if loading}
      <div class="msg">加载中…</div>
    {:else if error}
      <div class="msg error">{error}</div>
    {:else}
      <div class="list">
        {#each items as it, i (it.c.oid)}
          {@const target = it.action === "squash" ? squashTarget(i) : null}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="item a-{it.action}"
            class:over={overIdx === i && dragIdx !== i}
            ondragover={(e) => {
              if (dragIdx === null) return;
              e.preventDefault();
              overIdx = i;
            }}
            ondragleave={() => overIdx === i && (overIdx = null)}
            ondrop={(e) => {
              e.preventDefault();
              drop(i);
            }}
          >
            <span
              class="grip"
              draggable="true"
              role="button"
              tabindex="-1"
              title="拖动调整顺序"
              ondragstart={(e) => {
                dragIdx = i;
                e.dataTransfer?.setData("text/plain", it.c.oid);
              }}
              ondragend={() => (dragIdx = overIdx = null)}>⋮⋮</span
            >
            <div class="acts">
              <button class:on={it.action === "pick"} onclick={() => setAction(it, "pick")} title="保留这个提交">保留</button>
              <button class:on={it.action === "squash"} onclick={() => setAction(it, "squash")} title="把这个提交的改动并进下面最近的保留提交">合并到下面</button>
              <button class:on={it.action === "drop"} class="drop" onclick={() => setAction(it, "drop")} title="删除这个提交和它的改动">删除</button>
            </div>
            <div class="main">
              {#if editing === it.c.oid && it.action === "pick"}
                <textarea
                  rows={Math.min(12, Math.max(3, messageOf(it).split("\n").length + 1))}
                  value={messageOf(it)}
                  oninput={(e) => (it.edited = (e.target as HTMLTextAreaElement).value)}
                  onkeydown={(e) => e.key === "Escape" && (e.stopPropagation(), (editing = null))}
                ></textarea>
                <div class="erow">
                  <button class="small" onclick={() => (editing = null)}>完成</button>
                  {#if it.edited !== null}<button class="small" onclick={() => (it.edited = null)}>还原说明</button>{/if}
                </div>
              {:else}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <div
                  class="summary"
                  class:clickable={it.action === "pick"}
                  onclick={() => it.action === "pick" && (editing = it.c.oid)}
                  title={it.action === "pick" ? "点击修改提交说明" : ""}
                >
                  {it.action === "pick" ? messageOf(it).split("\n")[0] : it.c.summary}
                  {#if it.edited !== null}<span class="badge">已改说明</span>{/if}
                  {#if (groups.get(it.c.oid)?.length ?? 0) > 0}<span class="badge">合并了 {groups.get(it.c.oid)!.length} 个</span>{/if}
                  {#if it.c.parents > 1}<span class="badge warn">合并提交</span>{/if}
                </div>
                <div class="meta">
                  <span class="mono">{it.c.oid.slice(0, 7)}</span>
                  <span>{it.c.author}</span>
                  <span>{relativeTime(it.c.time)}</span>
                  {#if it.action === "squash"}
                    <span class="into">{target ? `→ 并入 ${target.c.oid.slice(0, 7)}` : "下面没有可以并入的提交"}</span>
                  {/if}
                </div>
              {/if}
            </div>
            <div class="move">
              <button class="small" disabled={i === 0} onclick={() => move(i, -1)} title="上移">↑</button>
              <button class="small" disabled={i === items.length - 1} onclick={() => move(i, 1)} title="下移">↓</button>
            </div>
          </div>
        {/each}
        <div class="base">⟂ 起点：{from.slice(0, 7)} 的父提交（不会改变）</div>
      </div>
    {/if}

    {#if problem && !loading && !error}<div class="problem">{problem}</div>{/if}
    <div class="buttons">
      <button onclick={reset} disabled={!changed}>全部还原</button>
      <span class="grow"></span>
      <span class="count">{items.length} 个提交</span>
      <button onclick={close}>取消</button>
      <button class="primary" disabled={!!problem || !changed || running || loading} onclick={start}>开始变基</button>
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
    padding-top: 6vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 780px;
    max-width: calc(100vw - 32px);
    max-height: 86vh;
    padding: 16px 20px 18px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
  }
  .title {
    font-size: 15px;
    font-weight: 600;
  }
  .sub {
    margin-top: 4px;
    color: var(--text-dim);
    font-size: 12px;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .item {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }
  .item.over {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .item.a-drop .summary,
  .item.a-drop .meta {
    text-decoration: line-through;
    opacity: 0.5;
  }
  .item.a-squash {
    padding-left: 30px;
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }
  .grip {
    padding-top: 4px;
    color: var(--text-faint);
    cursor: grab;
    user-select: none;
  }
  .acts {
    display: flex;
    flex-shrink: 0;
    border: 1px solid var(--border);
    border-radius: 5px;
    overflow: hidden;
  }
  .acts button {
    height: 24px;
    padding: 0 8px;
    border: 0;
    border-radius: 0;
    font-size: 12px;
  }
  .acts button.on {
    background: var(--accent);
    color: #fff;
  }
  .acts button.drop.on {
    background: var(--del);
  }
  .main {
    flex: 1;
    min-width: 0;
  }
  .summary {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 13px;
  }
  .summary.clickable {
    cursor: text;
  }
  .summary.clickable:hover {
    text-decoration: underline dotted;
  }
  .badge {
    margin-left: 6px;
    padding: 0 5px;
    border-radius: 3px;
    background: var(--bg-hover);
    color: var(--text-dim);
    font-size: 11px;
  }
  .badge.warn {
    color: var(--warn);
  }
  .meta {
    display: flex;
    gap: 10px;
    margin-top: 2px;
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  .into {
    color: var(--accent);
  }
  textarea {
    width: 100%;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12.5px;
    resize: vertical;
  }
  .erow {
    display: flex;
    gap: 6px;
    margin-top: 4px;
  }
  .move {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
  }
  .small {
    height: 24px;
    padding: 0 7px;
    font-size: 12px;
  }
  .base {
    padding: 8px 12px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .problem {
    color: var(--warn);
    font-size: 12.5px;
  }
  .msg {
    padding: 30px;
    text-align: center;
    color: var(--text-dim);
  }
  .msg.error {
    color: var(--del);
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
  .count {
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
