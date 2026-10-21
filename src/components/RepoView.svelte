<script lang="ts">
  import { t } from "../lib/i18n";
  import type { RefInfo, Remote, Row, Stash } from "../lib/api";
  import { addRemote, addSubmodule, addWorktree, app, confirm, copyRemoteUrl, createPull, dialog, errorText, mergeBranch, openFlow, openRemoteWeb, openRepo, toast, type RepoTab } from "../lib/store.svelte";
  import { api, type Submodule, type Worktree } from "../lib/api";
  import { shareUrl } from "../lib/util";
  import Sidebar from "./Sidebar.svelte";
  import CommitGraph from "./CommitGraph.svelte";
  import CommitPanel from "./CommitPanel.svelte";
  import WorkingTree from "./WorkingTree.svelte";
  import DiffView from "./DiffView.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import StashPanel from "./StashPanel.svelte";
  import ConflictView from "./ConflictView.svelte";
  import FileView from "./FileView.svelte";
  import CompareView from "./CompareView.svelte";
  import ReflogView from "./ReflogView.svelte";
  import PullsView from "./PullsView.svelte";
  import BisectBar from "./BisectBar.svelte";
  import PickedPanel from "./PickedPanel.svelte";

  let { tab, active }: { tab: RepoTab; active: boolean } = $props();

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let sideW = $state(250);
  let panelW = $state(420);

  const current = $derived(tab.repo?.head.branch ?? "HEAD");

  function copy(text: string) {
    navigator.clipboard.writeText(text).then(
      () => toast(t("已复制")),
      () => toast(t("复制失败"), "error"),
    );
  }

  export async function createBranchAt(start: string, label: string) {
    const r = await dialog({
      title: t("新建分支"),
      message: t("从 {0} 创建", label),
      input: { placeholder: t("分支名称"), value: "" },
      checkbox: { label: t("创建后切换到新分支"), value: true },
      confirmText: t("创建"),
    });
    if (r?.input.trim()) await tab.run({ op: "create_branch", name: r.input.trim(), start, checkout: r.checkbox }, t("正在创建分支…"));
  }

  async function createTagAt(oid: string) {
    const r = await dialog({
      title: t("新建标签"),
      message: t("在提交 {0} 上创建标签", oid.slice(0, 7)),
      input: { placeholder: t("标签名称，例如 v1.0.0"), value: "" },
      confirmText: t("下一步"),
    });
    if (!r?.input.trim()) return;
    const m = await dialog({
      title: t("标签说明"),
      message: t("填写说明会创建附注标签，留空则创建轻量标签"),
      input: { placeholder: t("说明（可选）"), value: "", multiline: true },
      confirmText: t("创建"),
    });
    if (m) await tab.run({ op: "create_tag", name: r.input.trim(), oid, message: m.input || null });
  }

  // ---------- 远程仓库 ----------

  async function editRemoteUrl(r: Remote) {
    const d = await dialog({
      title: t("修改 {0} 的地址", r.name),
      input: { placeholder: "git@gitee.com:user/repo.git", value: r.url },
      confirmText: t("保存"),
    });
    if (d?.input.trim() && d.input.trim() !== r.url) await tab.run({ op: "set_remote_url", name: r.name, url: d.input.trim() });
  }

  async function renameRemote(r: Remote) {
    const d = await dialog({ title: t("重命名远程仓库"), input: { placeholder: t("远程仓库名称"), value: r.name }, confirmText: t("保存") });
    if (d?.input.trim() && d.input.trim() !== r.name) await tab.run({ op: "rename_remote", old: r.name, new: d.input.trim() });
  }

  async function removeRemote(r: Remote) {
    if (await confirm(t("删除远程仓库"), t("确定删除远程仓库 {0} 吗？\n只是删除本地的配置，不会影响服务器上的仓库。", r.name), t("删除"), true))
      await tab.run({ op: "remove_remote", name: r.name });
  }

  function onRemoteMenu(e: MouseEvent, name: string | null) {
    const r = tab.remotes.find((x) => x.name === name);
    const items: MenuItem[] = [];
    if (r) {
      items.push(
        { label: t("复制仓库地址"), hint: shareUrl(r.url), action: () => copyRemoteUrl(r) },
        { label: t("在浏览器中打开"), action: () => openRemoteWeb(r) },
        "sep",
        { label: t("修改地址…"), action: () => editRemoteUrl(r) },
        { label: t("重命名…"), action: () => renameRemote(r) },
        { label: t("删除远程仓库 {0}…", r.name), danger: true, action: () => removeRemote(r) },
        "sep",
      );
    }
    items.push({ label: t("添加远程仓库…"), action: () => addRemote(tab) });
    menu = { x: e.clientX, y: e.clientY, items };
  }

  async function renameBranch(r: RefInfo) {
    const d = await dialog({
      title: t("重命名分支"),
      message: r.upstream ? t("{0} 跟踪着远程分支 {1}", r.name, r.upstream) : undefined,
      input: { placeholder: t("新的分支名称"), value: r.name },
      // 有上游时才问要不要连远程一起改
      checkbox: r.upstream ? { label: t("同时重命名远程分支（推送新名字，删除远程的旧名字）"), value: false } : undefined,
      confirmText: t("重命名分支"),
    });
    const name = d?.input.trim();
    if (!d || !name || name === r.name) return;
    const remote = !!r.upstream && d.checkbox;
    await tab.run({ op: "rename_branch", old: r.name, new: name, remote }, remote ? t("正在重命名并推送…") : t("正在重命名…"));
  }

  async function checkoutRemote(r: RefInfo) {
    const short = r.name.slice(r.name.indexOf("/") + 1);
    const existing = tab.repo?.refs.find((x) => x.kind === "local" && x.name === short);
    if (existing) {
      if (!existing.is_head) await tab.run({ op: "checkout", name: short }, t("正在切换到 {0}…", short));
      return;
    }
    const d = await dialog({
      title: t("检出远程分支"),
      message: t("创建本地分支并跟踪 {0}", r.name),
      input: { placeholder: t("本地分支名称"), value: short },
      confirmText: t("检出"),
    });
    if (d?.input.trim()) await tab.run({ op: "checkout_remote", remote: r.name, local: d.input.trim() }, t("正在检出…"));
  }

  function onRefDblClick(r: RefInfo) {
    if (r.kind === "local" && !r.is_head) tab.run({ op: "checkout", name: r.name }, t("正在切换到 {0}…", r.name));
    else if (r.kind === "remote") checkoutRemote(r);
  }

  function mergeInto(target: string) {
    return mergeBranch(tab, target);
  }

  async function rebaseOnto(onto: string) {
    if (await confirm(t("变基"), t("将当前分支 {0} 变基到 {1}？\n变基会改写当前分支的提交历史。", current, onto), t("变基")))
      await tab.run({ op: "rebase", onto }, t("正在变基…"));
  }

  async function deleteBranch(r: RefInfo) {
    const d = await dialog({
      title: t("删除分支"),
      message: t("确定删除本地分支 {0} 吗？", r.name),
      checkbox: { label: t("强制删除（即使还没有合并）"), value: false },
      confirmText: t("删除"),
      danger: true,
    });
    if (d) await tab.run({ op: "delete_branch", name: r.name, force: d.checkbox });
  }

  // ---------- 工作树 ----------

  function onWorktreeMenu(e: MouseEvent, w: Worktree) {
    const items: MenuItem[] = [
      { label: t("在新标签页打开"), disabled: w.current, action: () => openRepo(w.path) },
      { label: isMacLike ? t("在访达中显示") : t("在资源管理器中显示"), action: () => openPath(w.path) },
      { label: t("复制路径"), action: () => copy(w.path) },
    ];
    if (!w.main) {
      items.push("sep", {
        label: t("删除工作树…"),
        danger: true,
        disabled: w.current,
        action: async () => {
          const d = await dialog({
            title: t("删除工作树"),
            message: t("删除 {0}？\n文件夹会被删掉，分支 {1} 本身不受影响。", w.path, w.branch ?? t("（分离头指针）")),
            checkbox: { label: t("即使有没提交的改动也删除"), value: false },
            confirmText: t("删除"),
            danger: true,
          });
          if (d) await tab.run({ op: "remove_worktree", path: w.path, force: d.checkbox }, t("正在删除工作树…"));
        },
      });
    }
    if (tab.extras?.worktrees.some((x) => x.prunable)) items.push("sep", { label: t("清理已经不存在的工作树"), action: () => tab.run({ op: "prune_worktrees" }) });
    items.push("sep", { label: t("新建工作树…"), action: () => addWorktree(tab) });
    menu = { x: e.clientX, y: e.clientY, items };
  }

  const isMacLike = navigator.platform.toLowerCase().includes("mac");
  function openPath(path: string) {
    api.revealPath(path).catch((e) => toast(errorText(e), "error"));
  }

  // ---------- 子模块 ----------

  function onSubmoduleMenu(e: MouseEvent, sm: Submodule | null) {
    const items: MenuItem[] = [];
    if (sm) {
      const full = `${tab.id.replace(/[\\/]+$/, "")}/${sm.path}`;
      items.push(
        { label: t("在新标签页打开"), disabled: sm.state === "uninitialized", action: () => openRepo(full) },
        {
          label: sm.state === "uninitialized" ? t("初始化并下载") : t("更新到记录的提交"),
          hint: "submodule update",
          action: () => tab.run({ op: "submodule_update", path: sm.path }, t("正在更新子模块…")),
        },
        {
          label: t("更新到远程最新的提交"),
          hint: "--remote",
          action: async () => {
            if (await confirm(t("更新子模块"), t("把 {0} 更新到它远程分支上最新的提交？\n更新后父仓库里会出现一个改动，需要提交。", sm.path), t("更新")))
              await tab.run({ op: "submodule_update_remote", path: sm.path }, t("正在更新子模块…"));
          },
        },
        { label: t("同步地址"), hint: "submodule sync", action: () => tab.run({ op: "submodule_sync", path: sm.path }) },
        "sep",
        { label: t("复制地址"), hint: sm.url, action: () => copy(sm.url) },
        { label: t("复制路径"), action: () => copy(sm.path) },
        "sep",
      );
    }
    items.push(
      { label: t("更新全部子模块"), action: () => tab.run({ op: "submodule_update", path: null }, t("正在更新子模块…")) },
      { label: t("添加子模块…"), action: () => addSubmodule(tab) },
    );
    menu = { x: e.clientX, y: e.clientY, items };
  }

  function onRefMenu(e: MouseEvent, r: RefInfo) {
    const items: MenuItem[] = [];
    if (r.kind !== "tag") {
      const shown = tab.focus.includes(r.full);
      items.push({ label: t("只显示这个分支"), action: () => tab.setFocus([r.full]) });
      if (tab.focus.length && !shown && !(tab.focus.length === 1 && tab.focus[0] === "HEAD"))
        items.push({ label: t("同时显示这个分支"), action: () => tab.setFocus([...tab.focus, r.full]) });
      if (shown && tab.focus.length > 1)
        items.push({ label: t("不再显示这个分支"), action: () => tab.setFocus(tab.focus.filter((f) => f !== r.full)) });
      if (tab.focus.length) items.push({ label: t("显示全部分支"), action: () => tab.setFocus([]) });
      items.push("sep");
    }
    const fb = r.kind === "local" ? tab.flowBranch(r.name) : null;
    if (fb) {
      items.push(
        { label: t("Git Flow：完成 {0} {1}…", fb.kind, fb.name), action: () => openFlow({ tab, mode: "finish", ...fb }) },
        { label: t("Git Flow：推送到远程"), action: () => tab.run({ op: "flow_publish", ...fb }, t("正在推送…")) },
        "sep",
      );
    }
    if (r.kind === "local") {
      items.push(
        { label: t("检出 {0}", r.name), disabled: r.is_head, action: () => tab.run({ op: "checkout", name: r.name }, t("正在切换分支…")) },
        "sep",
        { label: t("将 {0} 合并到 {1}", r.name, current), disabled: r.is_head, action: () => mergeInto(r.name) },
        { label: t("将 {0} 变基到 {1}", current, r.name), disabled: r.is_head, action: () => rebaseOnto(r.name) },
        "sep",
        { label: t("从这里新建分支…"), action: () => createBranchAt(r.name, r.name) },
        { label: t("复制分支名"), action: () => copy(r.name) },
        { label: t("与当前分支对比"), disabled: r.is_head, action: () => tab.openCompare(current, r.name) },
        { label: t("在新工作树中检出…"), disabled: r.is_head, action: () => addWorktree(tab, r.name) },
        { label: t("创建 Pull Request…"), disabled: !tab.remotes.length, action: () => createPull(tab, r.name) },
        "sep",
        { label: t("重命名…"), action: () => renameBranch(r) },
        { label: t("删除分支 {0}…", r.name), danger: true, disabled: r.is_head, action: () => deleteBranch(r) },
      );
    } else if (r.kind === "remote") {
      items.push(
        { label: t("检出为本地分支…"), action: () => checkoutRemote(r) },
        "sep",
        { label: t("将 {0} 合并到 {1}", r.name, current), action: () => mergeInto(r.name) },
        { label: t("将 {0} 变基到 {1}", current, r.name), action: () => rebaseOnto(r.name) },
        "sep",
        { label: t("从这里新建分支…"), action: () => createBranchAt(r.name, r.name) },
        { label: t("复制分支名"), action: () => copy(r.name) },
        { label: t("与当前分支对比"), disabled: r.is_head, action: () => tab.openCompare(current, r.name) },
        "sep",
        {
          label: t("删除远程分支 {0}…", r.name),
          danger: true,
          action: async () => {
            if (await confirm(t("删除远程分支"), t("确定从远程仓库删除 {0} 吗？其他人将无法再获取这个分支。", r.name), t("删除"), true))
              await tab.run({ op: "delete_remote_branch", name: r.name }, t("正在删除远程分支…"));
          },
        },
      );
    } else {
      items.push(
        { label: t("检出此标签（分离头指针）"), action: () => tab.run({ op: "checkout_detached", oid: r.oid }, t("正在检出…")) },
        { label: t("从这里新建分支…"), action: () => createBranchAt(r.oid, r.name) },
        { label: t("复制标签名"), action: () => copy(r.name) },
        { label: t("与当前分支对比"), action: () => tab.openCompare(current, r.name) },
        "sep",
        ...tab.remotes.map((rm) => ({
          label: t("推送到 {0}", rm.name),
          action: () => tab.run({ op: "push_tag", remote: rm.name, tag: r.name }, t("正在推送标签 {0}…", r.name)),
        })),
        ...tab.remotes.map((rm) => ({
          label: t("从 {0} 删除这个标签…", rm.name),
          danger: true,
          action: async () => {
            if (await confirm(t("删除远程标签"), t("确定从 {0} 删除标签 {1} 吗？其他人将无法再获取这个标签。", rm.name, r.name), t("删除"), true))
              await tab.run({ op: "delete_remote_tag", remote: rm.name, tag: r.name }, t("正在删除远程标签…"));
          },
        })),
        "sep",
        {
          label: t("删除标签 {0}", r.name),
          danger: true,
          action: async () => {
            if (await confirm(t("删除标签"), t("确定删除本地标签 {0} 吗？", r.name), t("删除"), true))
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
      if (await confirm(title, msg, t("确定"))) await op();
    };
    if (dst.is_head) {
      items.push(
        { label: t("把 {0} 合并到 {1}…", src.name, dst.name), action: () => mergeBranch(tab, src.name) },
        { label: t("把 {0} 变基到 {1}", dst.name, src.name), action: ask(t("变基"), t("将当前分支 {0} 变基到 {1}？\n变基会改写 {2} 的提交历史。", dst.name, src.name, dst.name), () => tab.run({ op: "rebase", onto: src.name }, t("正在变基…"))) },
      );
    } else {
      if (src.is_head)
        items.push({ label: t("把 {0} 变基到 {1}", src.name, dst.name), action: ask(t("变基"), t("将当前分支 {0} 变基到 {1}？\n变基会改写 {2} 的提交历史。", src.name, dst.name, src.name), () => tab.run({ op: "rebase", onto: dst.name }, t("正在变基…"))) });
      if (dst.kind === "local")
        items.push({ label: t("切换到 {0}，并把 {1} 合并进去…", dst.name, src.name), action: () => mergeBranch(tab, src.name, dst.name) });
      if (!src.is_head && src.kind === "local")
        items.push({ label: t("把 {0} 变基到 {1}（会切换到 {2}）", src.name, dst.name, src.name), action: ask(t("变基"), t("先切换到 {0}，再把它变基到 {1}？\n变基会改写 {2} 的提交历史。", src.name, dst.name, src.name), () => tab.run({ op: "rebase_branch", branch: src.name, onto: dst.name }, t("正在变基…"))) });
    }
    if (!items.length) {
      toast(t("不能把 {0} 合并到远程分支 {1}，请拖到本地分支上", src.name, dst.name), "error");
      return;
    }
    menu = { x: e.clientX, y: e.clientY, items };
  }

  function onRowMenu(e: MouseEvent, row: Row) {
    const short = row.oid.slice(0, 7);
    const reset = (mode: "soft" | "mixed" | "hard") => async () => {
      const desc = {
        soft: t("保留工作区和暂存区的改动"),
        mixed: t("保留工作区改动，清空暂存区"),
        hard: t("丢弃所有未提交的改动！此操作无法撤销"),
      }[mode];
      if (await confirm(t("重置 ({0})", mode), t("将 {0} 重置到 {1}？\n{2}", current, short, desc), t("重置"), mode === "hard"))
        await tab.run({ op: "reset", oid: row.oid, mode }, t("正在重置…"));
    };
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: t("检出此提交（分离头指针）"), action: () => tab.run({ op: "checkout_detached", oid: row.oid }, t("正在检出…")) },
        { label: t("在此新建分支…"), action: () => createBranchAt(row.oid, t("提交 {0}", short)) },
        { label: t("在此新建标签…"), action: () => createTagAt(row.oid) },
        "sep",
        {
          label: t("Cherry-pick 到 {0}", current),
          action: async () => {
            if (await confirm("Cherry-pick", t("把提交 {0} 应用到 {1}？", short, current), t("确定")))
              await tab.run({ op: "cherry_pick", oids: [row.oid] }, t("正在 cherry-pick…"));
          },
        },
        {
          label: t("撤销此提交 (revert)"),
          action: async () => {
            if (await confirm("Revert", t("创建一个新提交来撤销 {0} 的改动？", short), t("确定")))
              await tab.run({ op: "revert", oids: [row.oid] }, t("正在 revert…"));
          },
        },
        "sep",
        { label: t("与当前分支对比"), action: () => tab.openCompare(current, row.oid) },
        ...bisectItems(row.oid),
        {
          label: t("从这里开始交互式变基…"),
          hint: t("整理提交"),
          disabled: !tab.repo?.head.branch,
          action: () => (app.rebaseDialog = { tab, from: row.oid }),
        },
        "sep",
        { label: t("重置 {0} 到这里 - Soft", current), action: reset("soft") },
        { label: t("重置 {0} 到这里 - Mixed", current), action: reset("mixed") },
        { label: t("重置 {0} 到这里 - Hard", current), danger: true, action: reset("hard") },
        "sep",
        { label: t("复制提交哈希"), hint: short, action: () => copy(row.oid) },
        { label: t("复制提交说明"), action: () => copy(row.summary) },
      ],
    };
  }

  /** 多选提交的操作：右键菜单和右侧面板共用 */
  const pickedItems = $derived.by((): MenuItem[] => {
    const list = tab.picked;
    const n = list.length;
    // 列表按行号排，新的在前；拣选要从旧到新依次应用，还原从新到旧
    const newest = list.map((p) => p.oid);
    const oldest = [...newest].reverse();
    const done = () => {
      const r = tab.selectedRow;
      if (r !== null && tab.selectedOid) tab.select(r, tab.selectedOid);
      else tab.picked = [];
    };
    return [
      {
        label: t("Cherry-pick 到 {0}", current),
        action: async () => {
          if (await confirm("Cherry-pick", t("把这 {0} 个提交按从旧到新的顺序应用到 {1}？", n, current), t("确定")))
            if (await tab.run({ op: "cherry_pick", oids: oldest }, t("正在 cherry-pick…"))) done();
        },
      },
      {
        label: t("撤销这些提交 (revert)"),
        action: async () => {
          if (await confirm("Revert", t("从新到旧依次创建 {0} 个新提交来撤销它们的改动？", n), t("确定")))
            if (await tab.run({ op: "revert", oids: newest }, t("正在 revert…"))) done();
        },
      },
      "sep",
      {
        label: t("对比这两个提交"),
        disabled: n !== 2,
        hint: n !== 2 ? t("只能对比两个提交") : undefined,
        action: () => tab.openCompare(oldest[0], newest[0]),
      },
      "sep",
      { label: t("复制提交哈希"), action: () => copy(newest.join("\n")) },
      { label: t("复制提交说明"), action: () => copy(list.map((p) => p.summary).join("\n")) },
      "sep",
      { label: t("取消选择"), action: done },
    ];
  });

  function onPickedMenu(e: MouseEvent) {
    menu = { x: e.clientX, y: e.clientY, items: pickedItems };
  }

  /** 提交右键菜单里二分查错的选项 */
  function bisectItems(oid: string): MenuItem[] {
    const b = tab.bisect;
    const short = oid.slice(0, 7);
    if (b?.active) {
      return [
        "sep",
        { label: t("二分查错：标记为好的（没问题）"), action: () => tab.run({ op: "bisect_mark", verdict: "good", oid }, t("正在标记…")) },
        { label: t("二分查错：标记为坏的（有问题）"), action: () => tab.run({ op: "bisect_mark", verdict: "bad", oid }, t("正在标记…")) },
        { label: t("二分查错：跳过这个提交"), action: () => tab.run({ op: "bisect_mark", verdict: "skip", oid }, t("正在标记…")) },
      ];
    }
    return [
      {
        label: t("从这里开始二分查错…"),
        hint: t("找出哪个提交引入了 bug"),
        action: async () => {
          const ok = await confirm(
            t("二分查错"),
            t(
              "把 {0} 当作没问题的（好的）提交，当前的 HEAD 当作有问题的（坏的）提交。\ngit 会依次检出中间的提交，你测试后标记它好还是坏，几步之后就能找到第一个引入问题的提交。\n过程中会检出别的提交，请先提交或 stash 本地改动。",
              short,
            ),
            t("开始"),
          );
          if (ok) await tab.run({ op: "bisect_start", bad: "HEAD", good: oid }, t("正在开始二分查错…"));
        },
      },
    ];
  }

  function onStashMenu(e: MouseEvent, s: Stash) {
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: t("应用 (apply)"), action: () => tab.run({ op: "stash", action: "apply", index: s.index }, t("正在应用 stash…")) },
        { label: t("恢复并删除 (pop)"), action: () => tab.run({ op: "stash", action: "pop", index: s.index }, t("正在恢复 stash…")) },
        "sep",
        {
          label: t("删除"),
          danger: true,
          action: async () => {
            if (await confirm(t("删除 stash"), t("确定删除 “{0}” 吗？", s.message), t("删除"), true))
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
    <div class="err-title">{t("无法打开仓库")}</div>
    <div class="err">{tab.error}</div>
    <div class="err-path">{tab.requestedPath}</div>
    <button onclick={() => tab.open()}>{t("重试")}</button>
  </div>
{:else if !tab.repo}
  <div class="state"><div class="spinner"></div>{t("正在打开 {0}…", tab.name)}</div>
{:else}
  <div class="main" style="grid-template-columns: {sideW}px 1fr {panelW}px">
    <div class="side">
      <Sidebar {tab} {onRefMenu} {onRefDblClick} {onStashMenu} {onRefDrop} {onRemoteMenu} {onWorktreeMenu} {onSubmoduleMenu} />
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="handle right" onpointerdown={dragResize("side")}></div>
    </div>
    <div class="center">
      {#if tab.bisect?.active}<BisectBar {tab} state={tab.bisect} />{/if}
      <div class="graph-wrap" class:hidden={!!tab.diff || !!tab.fileView || !!tab.compareView || !!tab.toolView}>
        <CommitGraph {tab} {active} {onRowMenu} {onPickedMenu} {onRefMenu} {onRefDblClick} {onRefDrop} />
      </div>
      {#if tab.toolView === "reflog" && active}
        <div class="diff-wrap"><ReflogView {tab} onClose={() => (tab.toolView = null)} /></div>
      {:else if tab.toolView === "pulls" && active}
        <div class="diff-wrap"><PullsView {tab} onClose={() => (tab.toolView = null)} /></div>
      {:else if tab.compareView && active}
        <div class="diff-wrap">
          <CompareView {tab} view={tab.compareView} onClose={() => (tab.compareView = null)} />
        </div>
      {:else if tab.fileView && active}
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
      {:else if tab.picked.length > 1}
        <PickedPanel {tab} items={pickedItems} />
      {:else if tab.selectedOid}
        <CommitPanel {tab} oid={tab.selectedOid} />
      {:else}
        <div class="placeholder">{t("选择一个提交查看详情")}</div>
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
