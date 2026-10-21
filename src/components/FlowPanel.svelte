<script lang="ts">
  import { t } from "../lib/i18n";
  import type { FlowKind, RefInfo } from "../lib/api";
  import { openFlow, type RepoTab } from "../lib/store.svelte";

  let { tab, x, y, onClose }: { tab: RepoTab; x: number; y: number; onClose: () => void } = $props();

  const KINDS: { kind: FlowKind; label: string; desc: string }[] = [
    { kind: "feature", label: "Feature", desc: t("新功能") },
    { kind: "bugfix", label: "Bugfix", desc: t("开发中的问题修复") },
    { kind: "release", label: "Release", desc: t("发布新版本") },
    { kind: "hotfix", label: "Hotfix", desc: t("线上紧急修复") },
  ];

  const current = $derived(tab.flowBranch(tab.repo?.head.branch));

  /** 每种类型下已有的本地分支 */
  const groups = $derived(
    KINDS.map((k) => ({
      ...k,
      branches: (tab.repo?.refs ?? [])
        .filter((r) => r.kind === "local" && tab.flowBranch(r.name)?.kind === k.kind)
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((r) => ({ ref: r, name: tab.flowBranch(r.name)!.name })),
    })),
  );
  const total = $derived(groups.reduce((n, g) => n + g.branches.length, 0));

  function act(fn: () => void) {
    onClose();
    fn();
  }
  const start = (kind: FlowKind) => act(() => openFlow({ tab, mode: "start", kind }));
  const finish = (kind: FlowKind, name: string) => act(() => openFlow({ tab, mode: "finish", kind, name }));
  const publish = (kind: FlowKind, name: string) =>
    act(() => tab.run({ op: "flow_publish", kind, name }, t("正在推送…")));
  const checkout = (r: RefInfo) => act(() => tab.run({ op: "checkout", name: r.name }, t("正在切换到 {0}…", r.name)));
  const settings = () => act(() => openFlow({ tab, mode: "init" }));

  let el = $state<HTMLDivElement>();
  const left = $derived(Math.max(8, Math.min(x, window.innerWidth - (el?.offsetWidth ?? 420) - 8)));
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} onblur={onClose} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>
<div class="panel" bind:this={el} style="left:{left}px; top:{y}px" role="dialog">
  <div class="top">
    <span class="title">Git Flow</span>
    <span class="cfg">{tab.flow?.develop} → {tab.flow?.master}</span>
    <button class="link" onclick={settings}>{t("设置")}</button>
  </div>

  {#if current && tab.repo?.head.branch}
    {@const c = current}
    <div class="current">
      <div class="cur-label">{t("当前分支")}</div>
      <div class="cur-row">
        <span class="kind k-{c.kind}">{c.kind}</span>
        <span class="bname">{tab.repo.head.branch}</span>
        <span class="grow"></span>
        <button class="small" onclick={() => publish(c.kind, c.name)}>{t("推送")}</button>
        <button class="small primary" onclick={() => finish(c.kind, c.name)}>{t("完成")}</button>
      </div>
    </div>
  {/if}

  <div class="start">
    {#each KINDS as k (k.kind)}
      <button class="start-btn" onclick={() => start(k.kind)} title={t("开始一个新的 {0}（{1}）", k.label, k.desc)}>
        <span class="plus">＋</span>
        <span>
          <span class="sname">{t("开始 {0}", k.label)}</span>
          <span class="sdesc">{k.desc}</span>
        </span>
      </button>
    {/each}
  </div>

  <div class="list">
    {#if total === 0}
      <div class="empty">{t("还没有进行中的 feature / release / hotfix 分支")}</div>
    {/if}
    {#each groups as g (g.kind)}
      {#if g.branches.length}
        <div class="ghead">{g.label} <span class="count">{g.branches.length}</span></div>
        {#each g.branches as b (b.ref.full)}
          <div class="item" class:head={b.ref.is_head}>
            <span class="mark">{b.ref.is_head ? "✓" : ""}</span>
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="bname" ondblclick={() => !b.ref.is_head && checkout(b.ref)} title={t("{0}（双击切换）", b.ref.name)}>{b.name}</span>
            <span class="acts">
              {#if !b.ref.is_head}<button class="small" onclick={() => checkout(b.ref)}>{t("切换")}</button>{/if}
              <button class="small" onclick={() => publish(g.kind, b.name)}>{t("推送")}</button>
              <button class="small primary" onclick={() => finish(g.kind, b.name)}>{t("完成")}</button>
            </span>
          </div>
        {/each}
      {/if}
    {/each}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
  }
  .panel {
    position: fixed;
    z-index: 91;
    display: flex;
    flex-direction: column;
    width: 420px;
    max-height: 70vh;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
    font-size: 13px;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .title {
    font-weight: 600;
  }
  .cfg {
    flex: 1;
    color: var(--text-faint);
    font-size: 12px;
  }
  .link {
    height: 22px;
    padding: 0 8px;
    font-size: 12px;
  }
  .current {
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-hover);
  }
  .cur-label {
    margin-bottom: 6px;
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .cur-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .kind {
    padding: 1px 6px;
    border-radius: 3px;
    background: var(--accent);
    color: #fff;
    font-size: 11px;
  }
  .k-bugfix,
  .k-hotfix {
    background: var(--del);
  }
  .k-release {
    background: var(--add);
    color: #111;
  }
  .start {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .start-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    height: auto;
    padding: 6px 10px;
    text-align: left;
  }
  .plus {
    color: var(--accent);
    font-size: 15px;
  }
  .sname {
    display: block;
    font-weight: 600;
  }
  .sdesc {
    display: block;
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 0 8px;
  }
  .empty {
    padding: 14px;
    color: var(--text-faint);
    text-align: center;
  }
  .ghead {
    padding: 8px 14px 4px;
    color: var(--text-dim);
    font-size: 11.5px;
    font-weight: 700;
  }
  .count {
    margin-left: 4px;
    color: var(--text-faint);
    font-weight: normal;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 30px;
    padding: 0 10px 0 14px;
  }
  .item:hover {
    background: var(--bg-hover);
  }
  .item.head .bname {
    color: var(--accent);
    font-weight: 600;
  }
  .mark {
    width: 14px;
    flex-shrink: 0;
    color: var(--add);
    font-size: 11px;
  }
  .bname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .acts {
    display: flex;
    gap: 4px;
    opacity: 0.55;
  }
  .item:hover .acts {
    opacity: 1;
  }
  .grow {
    flex: 1;
  }
  .small {
    height: 24px;
    padding: 0 8px;
    font-size: 12px;
  }
</style>
