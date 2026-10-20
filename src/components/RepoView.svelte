<script lang="ts">
  import type { RefInfo, Row, Stash } from "../lib/api";
  import { app, confirm, dialog, openFlow, toast, type RepoTab } from "../lib/store.svelte";
  import Sidebar from "./Sidebar.svelte";
  import CommitGraph from "./CommitGraph.svelte";
  import CommitPanel from "./CommitPanel.svelte";
  import WorkingTree from "./WorkingTree.svelte";
  import DiffView from "./DiffView.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import StashPanel from "./StashPanel.svelte";
  import ConflictView from "./ConflictView.svelte";
  import FileView from "./FileView.svelte";

  let { tab, active }: { tab: RepoTab; active: boolean } = $props();

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let sideW = $state(250);
  let panelW = $state(420);

  const current = $derived(tab.repo?.head.branch ?? "HEAD");

  function copy(text: string) {
    navigator.clipboard.writeText(text).then(
      () => toast("已复制"),
      () => toast("复制失败", "error"),
    );
  }

  export async function createBranchAt(start: string, label: string) {
    const r = await dialog({
      title: "新建分支",
      message: `从 ${label} 创建`,
      input: { placeholder: "分支名称", value: "" },
      checkbox: { label: "创建后切换到新分支", value: true },
      confirmText: "创建",
    });
    if (r?.input.trim()) await tab.run({ op: "create_branch", name: r.input.trim(), start, checkout: r.checkbox }, "正在创建分支…");
  }

  async function createTagAt(oid: string) {
    const r = await dialog({
      title: "新建标签",
      message: `在提交 ${oid.slice(0, 7)} 上创建标签`,
      input: { placeholder: "标签名称，例如 v1.0.0", value: "" },
      confirmText: "下一步",
    });
    if (!r?.input.trim()) return;
    const m = await dialog({
      title: "标签说明",
      message: "填写说明会创建附注标签，留空则创建轻量标签",
      input: { placeholder: "说明（可选）", value: "", multiline: true },
      confirmText: "创建",
    });
    if (m) await tab.run({ op: "create_tag", name: r.input.trim(), oid, message: m.input || null });
  }

  async function checkoutRemote(r: RefInfo) {
    const short = r.name.slice(r.name.indexOf("/") + 1);
    const existing = tab.repo?.refs.find((x) => x.kind === "local" && x.name === short);
    if (existing) {
      if (!existing.is_head) await tab.run({ op: "checkout", name: short }, `正在切换到 ${short}…`);
      return;
    }
    const d = await dialog({
      title: "检出远程分支",
      message: `创建本地分支并跟踪 ${r.name}`,
      input: { placeholder: "本地分支名称", value: short },
      confirmText: "检出",
    });
    if (d?.input.trim()) await tab.run({ op: "checkout_remote", remote: r.name, local: d.input.trim() }, "正在检出…");
  }

  function onRefDblClick(r: RefInfo) {
    if (r.kind === "local" && !r.is_head) tab.run({ op: "checkout", name: r.name }, `正在切换到 ${r.name}…`);
    else if (r.kind === "remote") checkoutRemote(r);
  }

  async function mergeInto(target: string) {
    if (await confirm("合并", `将 ${target} 合并到当前分支 ${current}？`, "合并"))
      await tab.run({ op: "merge", target }, "正在合并…");
  }

  async function rebaseOnto(onto: string) {
    if (await confirm("变基", `将当前分支 ${current} 变基到 ${onto}？\n变基会改写当前分支的提交历史。`, "变基"))
      await tab.run({ op: "rebase", onto }, "正在变基…");
  }

  async function deleteBranch(r: RefInfo) {
    const d = await dialog({
      title: "删除分支",
      message: `确定删除本地分支 ${r.name} 吗？`,
      checkbox: { label: "强制删除（即使还没有合并）", value: false },
      confirmText: "删除",
      danger: true,
    });
    if (d) await tab.run({ op: "delete_branch", name: r.name, force: d.checkbox });
  }

  function onRefMenu(e: MouseEvent, r: RefInfo) {
    const items: MenuItem[] = [];
    if (r.kind !== "tag") {
      const shown = tab.focus.includes(r.full);
      items.push({ label: "只显示这个分支", action: () => tab.setFocus([r.full]) });
      if (tab.focus.length && !shown && !(tab.focus.length === 1 && tab.focus[0] === "HEAD"))
        items.push({ label: "同时显示这个分支", action: () => tab.setFocus([...tab.focus, r.full]) });
      if (shown && tab.focus.length > 1)
        items.push({ label: "不再显示这个分支", action: () => tab.setFocus(tab.focus.filter((f) => f !== r.full)) });
      if (tab.focus.length) items.push({ label: "显示全部分支", action: () => tab.setFocus([]) });
      items.push("sep");
    }
    const fb = r.kind === "local" ? tab.flowBranch(r.name) : null;
    if (fb) {
      items.push(
        { label: `Git Flow：完成 ${fb.kind} ${fb.name}…`, action: () => openFlow({ tab, mode: "finish", ...fb }) },
        { label: `Git Flow：推送到远程`, action: () => tab.run({ op: "flow_publish", ...fb }, "正在推送…") },
        "sep",
      );
    }
    if (r.kind === "local") {
      items.push(
        { label: `检出 ${r.name}`, disabled: r.is_head, action: () => tab.run({ op: "checkout", name: r.name }, "正在切换分支…") },
        "sep",
        { label: `将 ${r.name} 合并到 ${current}`, disabled: r.is_head, action: () => mergeInto(r.name) },
        { label: `将 ${current} 变基到 ${r.name}`, disabled: r.is_head, action: () => rebaseOnto(r.name) },
        "sep",
        { label: "从这里新建分支…", action: () => createBranchAt(r.name, r.name) },
        { label: "复制分支名", action: () => copy(r.name) },
        "sep",
        { label: `删除分支 ${r.name}…`, danger: true, disabled: r.is_head, action: () => deleteBranch(r) },
      );
    } else if (r.kind === "remote") {
      items.push(
        { label: "检出为本地分支…", action: () => checkoutRemote(r) },
        "sep",
        { label: `将 ${r.name} 合并到 ${current}`, action: () => mergeInto(r.name) },
        { label: `将 ${current} 变基到 ${r.name}`, action: () => rebaseOnto(r.name) },
        "sep",
        { label: "从这里新建分支…", action: () => createBranchAt(r.name, r.name) },
        { label: "复制分支名", action: () => copy(r.name) },
        "sep",
        {
          label: `删除远程分支 ${r.name}…`,
          danger: true,
          action: async () => {
            if (await confirm("删除远程分支", `确定从远程仓库删除 ${r.name} 吗？其他人将无法再获取这个分支。`, "删除", true))
              await tab.run({ op: "delete_remote_branch", name: r.name }, "正在删除远程分支…");
          },
        },
      );
    } else {
      items.push(
        { label: "检出此标签（分离头指针）", action: () => tab.run({ op: "checkout_detached", oid: r.oid }, "正在检出…") },
        { label: "从这里新建分支…", action: () => createBranchAt(r.oid, r.name) },
        { label: "复制标签名", action: () => copy(r.name) },
        "sep",
        {
          label: `删除标签 ${r.name}`,
          danger: true,
          action: async () => {
            if (await confirm("删除标签", `确定删除本地标签 ${r.name} 吗？`, "删除", true))
              await tab.run({ op: "delete_tag", name: r.name });
          },
        },
      );
    }
    menu = { x: e.clientX, y: e.clientY, items };
  }

  /** 把分支 src 拖到分支 dst 上：弹出合并 / 变基的选项 */
  function onRefDrop(e: MouseEvent, src: RefInfo, dst: RefInfo) {
    if (src.full === dst.full || src.kind === "tag" || dst.kind === "tag") return;
    const items: MenuItem[] = [];
    const ask = (title: string, msg: string, op: () => Promise<unknown>) => async () => {
      if (await confirm(title, msg, "确定")) await op();
    };
    if (dst.is_head) {
      items.push(
        { label: `把 ${src.name} 合并到 ${dst.name}`, action: ask("合并", `将 ${src.name} 合并到当前分支 ${dst.name}？`, () => tab.run({ op: "merge", target: src.name }, "正在合并…")) },
        { label: `把 ${dst.name} 变基到 ${src.name}`, action: ask("变基", `将当前分支 ${dst.name} 变基到 ${src.name}？\n变基会改写 ${dst.name} 的提交历史。`, () => tab.run({ op: "rebase", onto: src.name }, "正在变基…")) },
      );
    } else {
      if (src.is_head)
        items.push({ label: `把 ${src.name} 变基到 ${dst.name}`, action: ask("变基", `将当前分支 ${src.name} 变基到 ${dst.name}？\n变基会改写 ${src.name} 的提交历史。`, () => tab.run({ op: "rebase", onto: dst.name }, "正在变基…")) });
      if (dst.kind === "local")
        items.push({ label: `切换到 ${dst.name}，并把 ${src.name} 合并进去`, action: ask("合并", `先切换到 ${dst.name}，再把 ${src.name} 合并进去？`, () => tab.run({ op: "merge_into", branch: dst.name, source: src.name }, "正在合并…")) });
      if (!src.is_head && src.kind === "local")
        items.push({ label: `把 ${src.name} 变基到 ${dst.name}（会切换到 ${src.name}）`, action: ask("变基", `先切换到 ${src.name}，再把它变基到 ${dst.name}？\n变基会改写 ${src.name} 的提交历史。`, () => tab.run({ op: "rebase_branch", branch: src.name, onto: dst.name }, "正在变基…")) });
    }
    if (!items.length) {
      toast(`不能把 ${src.name} 合并到远程分支 ${dst.name}，请拖到本地分支上`, "error");
      return;
    }
    menu = { x: e.clientX, y: e.clientY, items };
  }

  function onRowMenu(e: MouseEvent, row: Row) {
    const short = row.oid.slice(0, 7);
    const reset = (mode: "soft" | "mixed" | "hard") => async () => {
      const desc = {
        soft: "保留工作区和暂存区的改动",
        mixed: "保留工作区改动，清空暂存区",
        hard: "丢弃所有未提交的改动！此操作无法撤销",
      }[mode];
      if (await confirm(`重置 (${mode})`, `将 ${current} 重置到 ${short}？\n${desc}`, "重置", mode === "hard"))
        await tab.run({ op: "reset", oid: row.oid, mode }, "正在重置…");
    };
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "检出此提交（分离头指针）", action: () => tab.run({ op: "checkout_detached", oid: row.oid }, "正在检出…") },
        { label: "在此新建分支…", action: () => createBranchAt(row.oid, `提交 ${short}`) },
        { label: "在此新建标签…", action: () => createTagAt(row.oid) },
        "sep",
        {
          label: `Cherry-pick 到 ${current}`,
          action: async () => {
            if (await confirm("Cherry-pick", `把提交 ${short} 应用到 ${current}？`, "确定"))
              await tab.run({ op: "cherry_pick", oid: row.oid }, "正在 cherry-pick…");
          },
        },
        {
          label: "撤销此提交 (revert)",
          action: async () => {
            if (await confirm("Revert", `创建一个新提交来撤销 ${short} 的改动？`, "确定"))
              await tab.run({ op: "revert", oid: row.oid }, "正在 revert…");
          },
        },
        "sep",
        {
          label: `从这里开始交互式变基…`,
          hint: "整理提交",
          disabled: !tab.repo?.head.branch,
          action: () => (app.rebaseDialog = { tab, from: row.oid }),
        },
        "sep",
        { label: `重置 ${current} 到这里 - Soft`, action: reset("soft") },
        { label: `重置 ${current} 到这里 - Mixed`, action: reset("mixed") },
        { label: `重置 ${current} 到这里 - Hard`, danger: true, action: reset("hard") },
        "sep",
        { label: "复制提交哈希", hint: short, action: () => copy(row.oid) },
        { label: "复制提交说明", action: () => copy(row.summary) },
      ],
    };
  }

  function onStashMenu(e: MouseEvent, s: Stash) {
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "应用 (apply)", action: () => tab.run({ op: "stash", action: "apply", index: s.index }, "正在应用 stash…") },
        { label: "恢复并删除 (pop)", action: () => tab.run({ op: "stash", action: "pop", index: s.index }, "正在恢复 stash…") },
        "sep",
        {
          label: "删除",
          danger: true,
          action: async () => {
            if (await confirm("删除 stash", `确定删除 “${s.message}” 吗？`, "删除", true))
              await tab.run({ op: "stash", action: "drop", index: s.index });
          },
        },
      ],
    };
  }

  function dragResize(which: "side" | "panel") {
    return (e: PointerEvent) => {
      e.preventDefault();
      const startX = e.clientX;
      const start = which === "side" ? sideW : panelW;
      const move = (ev: PointerEvent) => {
        const dx = ev.clientX - startX;
        if (which === "side") sideW = Math.max(170, Math.min(480, start + dx));
        else panelW = Math.max(320, Math.min(900, start - dx));
      };
      const up = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    };
  }
</script>

{#if tab.error}
  <div class="state">
    <div class="err-title">无法打开仓库</div>
    <div class="err">{tab.error}</div>
    <div class="err-path">{tab.requestedPath}</div>
    <button onclick={() => tab.open()}>重试</button>
  </div>
{:else if !tab.repo}
  <div class="state"><div class="spinner"></div>正在打开 {tab.name}…</div>
{:else}
  <div class="main" style="grid-template-columns: {sideW}px 1fr {panelW}px">
    <div class="side">
      <Sidebar {tab} {onRefMenu} {onRefDblClick} {onStashMenu} {onRefDrop} />
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="handle right" onpointerdown={dragResize("side")}></div>
    </div>
    <div class="center">
      <div class="graph-wrap" class:hidden={!!tab.diff || !!tab.fileView}>
        <CommitGraph {tab} {active} {onRowMenu} {onRefMenu} {onRefDblClick} {onRefDrop} />
      </div>
      {#if tab.fileView && active}
        <div class="diff-wrap">
          <FileView {tab} view={tab.fileView} onClose={() => (tab.fileView = null)} />
        </div>
      {:else if tab.diff?.source === "conflict" && active}
        <div class="diff-wrap">
          <ConflictView {tab} path={tab.diff.path} onClose={() => (tab.diff = null)} />
        </div>
      {:else if tab.diff && active}
        <div class="diff-wrap">
          <DiffView {tab} target={tab.diff} onClose={() => (tab.diff = null)} />
        </div>
      {/if}
    </div>
    <div class="panel">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="handle left" onpointerdown={dragResize("panel")}></div>
      {#if tab.panel === "wip"}
        <WorkingTree {tab} />
      {:else if tab.panel === "stash" && tab.selectedStash}
        <StashPanel {tab} stash={tab.selectedStash} />
      {:else if tab.selectedOid}
        <CommitPanel {tab} oid={tab.selectedOid} />
      {:else}
        <div class="placeholder">选择一个提交查看详情</div>
      {/if}
    </div>
  </div>
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .main {
    display: grid;
    height: 100%;
    min-height: 0;
  }
  .side,
  .center,
  .panel {
    position: relative;
    min-width: 0;
    min-height: 0;
  }
  .center {
    display: flex;
    flex-direction: column;
  }
  .graph-wrap,
  .diff-wrap {
    flex: 1;
    min-height: 0;
  }
  .graph-wrap.hidden {
    display: none;
  }
  .panel {
    display: flex;
    flex-direction: column;
    background: var(--bg-side);
    border-left: 1px solid var(--border);
  }
  .placeholder {
    margin: auto;
    color: var(--text-dim);
  }
  .handle {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 6px;
    z-index: 5;
    cursor: col-resize;
  }
  .handle.right {
    right: -3px;
  }
  .handle.left {
    left: -3px;
  }
  .handle:hover {
    background: var(--accent);
  }
  .state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 100%;
    color: var(--text-dim);
  }
  .err-title {
    color: var(--text);
    font-size: 16px;
    font-weight: 600;
  }
  .err {
    max-width: 600px;
    color: var(--del);
    white-space: pre-wrap;
    text-align: center;
  }
  .err-path {
    color: var(--text-faint);
    font-size: 12px;
  }
  .spinner {
    width: 22px;
    height: 22px;
    border: 3px solid var(--border);
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
