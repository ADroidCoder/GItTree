<script lang="ts">
  import { t } from "../lib/i18n";
  import { api, type StatusEntry } from "../lib/api";
  import { confirm, dialog, errorText, openIn, toast, type RepoTab } from "../lib/store.svelte";
  import { isMac } from "../lib/util";
  import FileList, { type FileItem } from "./FileList.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  let { tab }: { tab: RepoTab } = $props();

  let summary = $state("");
  let description = $state("");
  let amend = $state(false);
  let committing = $state(false);
  let treeView = $state(false);

  const st = $derived(tab.status);

  function toItems(list: StatusEntry[], staged: boolean): FileItem[] {
    return list.map((e) => ({
      key: e.path,
      path: e.path,
      oldPath: e.old_path,
      status: staged ? e.index : e.worktree === "?" ? "?" : e.worktree,
    }));
  }

  const unstaged = $derived(st ? toItems(st.unstaged, false) : []);
  const staged = $derived(st ? toItems(st.staged, true) : []);
  const conflicted = $derived(
    st ? st.conflicted.map((e) => ({ key: e.path, path: e.path, oldPath: null, status: "U" })) : [],
  );

  function open(f: FileItem, isStaged: boolean) {
    tab.diff = { source: "workdir", path: f.path, oldPath: f.oldPath, staged: isStaged };
  }

  // ---------- 多选：⌘/Ctrl 点击切换，Shift 点击选一段；一次只在一个区域里选 ----------
  type Section = "unstaged" | "staged";
  let sel = $state<{ section: Section | null; keys: Set<string>; anchor: string | null }>({
    section: null,
    keys: new Set(),
    anchor: null,
  });
  const listOf = (sec: Section) => (sec === "unstaged" ? unstaged : staged);
  /** 状态刷新后，已经不在列表里的文件自动取消选中 */
  const selected = $derived.by(() => {
    if (!sel.section) return [] as FileItem[];
    return listOf(sel.section).filter((f) => sel.keys.has(f.key));
  });
  const selectedKeys = (sec: Section) => (sel.section === sec ? sel.keys : new Set<string>());

  function onSelect(sec: Section, f: FileItem, e: MouseEvent, visible: FileItem[]) {
    const mod = isMac ? e.metaKey : e.ctrlKey;
    if (mod) {
      const keys = new Set(sel.section === sec ? sel.keys : []);
      if (keys.has(f.key)) keys.delete(f.key);
      else keys.add(f.key);
      sel = { section: sec, keys, anchor: f.key };
      return;
    }
    if (e.shiftKey && sel.section === sec && sel.anchor) {
      const a = visible.findIndex((x) => x.key === sel.anchor);
      const b = visible.findIndex((x) => x.key === f.key);
      if (a >= 0 && b >= 0) {
        const [lo, hi] = a < b ? [a, b] : [b, a];
        sel = { section: sec, keys: new Set(visible.slice(lo, hi + 1).map((x) => x.key)), anchor: sel.anchor };
        return;
      }
    }
    sel = { section: sec, keys: new Set([f.key]), anchor: f.key };
    open(f, sec === "staged");
  }

  function clearSel() {
    sel = { section: null, keys: new Set(), anchor: null };
  }

  async function stashFiles(files: FileItem[]) {
    const r = await dialog({
      title: t("Stash 选中的文件"),
      message: files.length === 1 ? files[0].path : t("{0} 个文件，其他改动保留在工作区", files.length),
      input: { placeholder: t("说明（可选）"), value: "" },
      confirmText: "Stash",
    });
    if (!r) return;
    if (await tab.run({ op: "stash_push", message: r.input || null, paths: files.map((f) => f.path) }, t("正在 stash…"))) clearSel();
  }

  async function stageFiles(files: FileItem[]) {
    if (await tab.run({ op: "stage", paths: files.map((f) => f.path) })) clearSel();
  }

  async function unstageFiles(files: FileItem[]) {
    if (await tab.run({ op: "unstage", paths: files.map((f) => f.path) })) clearSel();
  }

  // ---------- 右键菜单：右键的文件在选中范围里时对所有选中的文件操作 ----------
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  function onContext(sec: Section, e: MouseEvent, f: FileItem) {
    let files = selected;
    if (sel.section !== sec || !sel.keys.has(f.key)) {
      sel = { section: sec, keys: new Set([f.key]), anchor: f.key };
      files = [f];
    }
    const n = files.length > 1 ? t(" {0} 个文件", files.length) : "";
    const items: MenuItem[] =
      sec === "unstaged"
        ? [
            { label: t("暂存{0}", n), action: () => stageFiles(files) },
            { label: `Stash${n}…`, action: () => stashFiles(files) },
            "sep",
            { label: t("丢弃{0}的改动…", n), danger: true, action: () => discard(files) },
          ]
        : [
            { label: t("取消暂存{0}", n), action: () => unstageFiles(files) },
            { label: `Stash${n}…`, action: () => stashFiles(files) },
          ];
    if (files.length === 1) {
      const p = files[0].path;
      if (files[0].status !== "D")
        items.push(
          "sep",
          { label: t("用编辑器打开"), action: () => openIn(tab, "editor", p) },
          { label: isMac ? t("在访达中显示") : t("在资源管理器中显示"), action: () => openIn(tab, "folder", p) },
        );
      if (files[0].status !== "?")
        items.push("sep", { label: t("文件历史"), action: () => tab.openFile(p, "history") }, { label: "Blame", action: () => tab.openFile(p, "blame") });
    }
    items.push("sep", {
      label: t("复制路径"),
      action: () => navigator.clipboard.writeText(files.map((x) => x.path).join("\n")).then(() => toast(t("已复制"))),
    });
    menu = { x: e.clientX, y: e.clientY, items };
  }

  function conflictMenu(e: MouseEvent, f: FileItem) {
    const p = f.path;
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: t("打开冲突解决…"), action: () => (tab.diff = { source: "conflict", path: p, oldPath: null }) },
        "sep",
        { label: t("整个文件用当前的版本"), action: () => tab.run({ op: "resolve_side", path: p, theirs: false }, t("正在解决冲突…")) },
        { label: t("整个文件用传入的版本"), action: () => tab.run({ op: "resolve_side", path: p, theirs: true }, t("正在解决冲突…")) },
        { label: t("按现在的内容标记为已解决"), action: () => tab.run({ op: "stage", paths: [p] }) },
        "sep",
        { label: t("用编辑器打开"), action: () => openIn(tab, "editor", p) },
        { label: t("文件历史"), action: () => tab.openFile(p, "history") },
        { label: t("复制路径"), action: () => navigator.clipboard.writeText(p).then(() => toast(t("已复制"))) },
      ],
    };
  }

  async function discard(paths: FileItem[]) {
    const names = paths.length === 1 ? paths[0].path : t("{0} 个文件", paths.length);
    if (!(await confirm(t("丢弃改动"), t("确定丢弃 {0} 的改动吗？未跟踪的文件会被删除，此操作无法撤销。", names), t("丢弃"), true)))
      return;
    await tab.run({
      op: "discard",
      tracked: paths.filter((p) => p.status !== "?").map((p) => p.path),
      untracked: paths.filter((p) => p.status === "?").map((p) => p.path),
    });
  }

  async function toggleAmend() {
    amend = !amend;
    if (amend && !summary.trim()) {
      try {
        const msg = (await api.lastCommitMessage(tab.id)).trim();
        const [first, ...rest] = msg.split("\n");
        summary = first;
        description = rest.join("\n").trim();
      } catch (e) {
        toast(errorText(e), "error");
      }
    }
  }

  async function commit() {
    if (!summary.trim()) return toast(t("请填写提交说明"), "error");
    if (!amend && staged.length === 0) return toast(t("没有已暂存的改动，请先暂存要提交的文件"), "error");
    committing = true;
    const message = description.trim() ? `${summary.trim()}\n\n${description.trim()}\n` : `${summary.trim()}\n`;
    const ok = await tab.run({ op: "commit", message, amend }, t("正在提交…"));
    committing = false;
    if (ok) {
      summary = "";
      description = "";
      amend = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter" && (isMac ? e.metaKey : e.ctrlKey)) {
      e.preventDefault();
      commit();
    }
  }

  const activePath = $derived(tab.diff?.source === "workdir" ? tab.diff.path : null);
  const activeStaged = $derived(tab.diff?.source === "workdir" ? !!tab.diff.staged : false);
</script>

<div class="wt">
  {#if st?.in_progress}
    <div class="progress">
      <div>{t("正在进行")} <b>{st.in_progress}</b>{st.conflicted.length ? t("，还有 {0} 个冲突文件", st.conflicted.length) : ""}</div>
      <div class="pbtns">
        <button onclick={() => tab.run({ op: "sequencer", kind: st!.in_progress!, action: "continue" }, t("继续中…"))}>{t("继续")}</button>
        <button class="danger" onclick={() => tab.run({ op: "sequencer", kind: st!.in_progress!, action: "abort" }, t("中止中…"))}>{t("中止")}</button>
      </div>
    </div>
  {/if}

  <div class="lists">
    <div class="toolbar">
      <span class="tip">{t("{0} / Shift + 点击可多选，右键查看更多操作", isMac ? "⌘" : "Ctrl")}</span>
      <button class="view-toggle" onclick={() => (treeView = !treeView)}>{treeView ? t("列表") : t("目录树")}</button>
    </div>
    {#if conflicted.length}
      <div class="sec-head conflict">
        <span>{t("冲突 ({0})", conflicted.length)}</span>
      </div>
      <FileList
        files={conflicted}
        tree={treeView}
        active={tab.diff?.source === "conflict" ? tab.diff.path : null}
        onOpen={(f) => (tab.diff = { source: "conflict", path: f.path, oldPath: null })}
        onContext={conflictMenu}
      >
        {#snippet actions(f)}
          <button onclick={(e) => { e.stopPropagation(); tab.diff = { source: "conflict", path: f.path, oldPath: null }; }} title={t("打开冲突解决界面")}>{t("解决")}</button>
        {/snippet}
      </FileList>
    {/if}

    <div class="sec-head">
      <span>{t("未暂存 ({0})", unstaged.length)}</span>
      <span class="sec-acts">
        {#if sel.section === "unstaged" && selected.length > 1}
          <span class="sel-count">{t("已选 {0}", selected.length)}</span>
          <button class="danger" onclick={() => discard(selected)}>{t("丢弃")}</button>
          <button onclick={() => stashFiles(selected)}>Stash</button>
          <button onclick={() => stageFiles(selected)}>{t("暂存所选")}</button>
        {:else if unstaged.length}
          <button class="danger" onclick={() => discard(unstaged)}>{t("全部丢弃")}</button>
          <button onclick={() => tab.run({ op: "stage_all" })}>{t("全部暂存")}</button>
        {/if}
      </span>
    </div>
    <FileList
      files={unstaged}
      tree={treeView}
      active={activeStaged ? null : activePath}
      onOpen={(f) => open(f, false)}
      selected={selectedKeys("unstaged")}
      onSelect={(f, e, visible) => onSelect("unstaged", f, e, visible)}
      onContext={(e, f) => onContext("unstaged", e, f)}
    >
      {#snippet actions(f)}
        <button class="danger" onclick={(e) => { e.stopPropagation(); discard([f]); }} title={t("丢弃改动")}>{t("丢弃")}</button>
        <button onclick={(e) => { e.stopPropagation(); tab.run({ op: "stage", paths: [f.path] }); }} title={t("暂存")}>{t("暂存")}</button>
      {/snippet}
    </FileList>

    <div class="sec-head">
      <span>{t("已暂存 ({0})", staged.length)}</span>
      <span class="sec-acts">
        {#if sel.section === "staged" && selected.length > 1}
          <span class="sel-count">{t("已选 {0}", selected.length)}</span>
          <button onclick={() => stashFiles(selected)}>Stash</button>
          <button onclick={() => unstageFiles(selected)}>{t("取消暂存所选")}</button>
        {:else if staged.length}
          <button onclick={() => tab.run({ op: "unstage_all" })}>{t("全部取消暂存")}</button>
        {/if}
      </span>
    </div>
    <FileList
      files={staged}
      tree={treeView}
      active={activeStaged ? activePath : null}
      onOpen={(f) => open(f, true)}
      selected={selectedKeys("staged")}
      onSelect={(f, e, visible) => onSelect("staged", f, e, visible)}
      onContext={(e, f) => onContext("staged", e, f)}
    >
      {#snippet actions(f)}
        <button onclick={(e) => { e.stopPropagation(); tab.run({ op: "unstage", paths: [f.path] }); }} title={t("取消暂存")}>{t("取消暂存")}</button>
      {/snippet}
    </FileList>
  </div>

  <div class="commit-box">
    <input class="summary" placeholder={t("提交说明（必填）")} bind:value={summary} onkeydown={onKey} maxlength="200" />
    <textarea placeholder={t("详细描述（可选）")} bind:value={description} onkeydown={onKey} rows="4"></textarea>
    <div class="commit-row">
      <label class="amend">
        <input type="checkbox" checked={amend} onchange={toggleAmend} />
        {t("修改上一次提交")}
      </label>
      <span class="count">{summary.length > 72 ? `${summary.length}/72` : ""}</span>
      <button class="primary" disabled={committing} onclick={commit} title={isMac ? "⌘ + Enter" : "Ctrl + Enter"}>
        {amend ? t("修改提交") : staged.length ? t("提交 {0} 个文件", staged.length) : t("提交")}
      </button>
    </div>
  </div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .wt {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .progress {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 14px;
    background: color-mix(in srgb, var(--warn) 18%, transparent);
    border-bottom: 1px solid var(--border);
    font-size: 13px;
  }
  .pbtns {
    display: flex;
    gap: 8px;
  }
  .lists {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 10px 0 12px;
  }
  .tip {
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .sel-count {
    color: var(--accent);
    font-weight: normal;
    text-transform: none;
  }
  .view-toggle {
    height: 22px;
    font-size: 12px;
  }
  .sec-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px 4px;
    color: var(--text-dim);
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
  }
  .sec-head.conflict {
    color: var(--del);
  }
  .sec-acts {
    display: flex;
    gap: 6px;
  }
  .sec-acts button,
  :global(.acts button) {
    height: 20px;
    padding: 0 7px;
    font-size: 11.5px;
    font-weight: normal;
    text-transform: none;
  }
  .commit-box {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-top: 1px solid var(--border);
  }
  .summary {
    font-weight: 600;
  }
  textarea {
    resize: vertical;
    min-height: 60px;
  }
  .commit-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .amend {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .count {
    flex: 1;
    color: var(--warn);
    font-size: 12px;
  }
</style>
