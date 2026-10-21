import { t } from "./i18n";
import { isMac, shareUrl, webUrl } from "./util";
import { prefs } from "./prefs";
import { autoArchive, loadTasks, saveTasks, type Board } from "./board";
import { notifyInBackground, notifySettings } from "./notify.svelte";
import { api, type AskPassRequest, type BisectState, type MergeMode, type FileDiff, type FlowConfig, type FlowKind, type Op, type OpenTarget, type ProgressEvent, type RefInfo, type Remote, type RepoExtras, type RepoInfo, type Row, type Stash, type Status } from "./api";

export interface DiffTarget {
  /** commit：某个提交里的文件；workdir：工作区文件；conflict：解决冲突；compare：两个提交之间 */
  source: "commit" | "workdir" | "conflict" | "compare";
  path: string;
  oldPath: string | null;
  oid?: string;
  staged?: boolean;
  /** compare：对比的两端（提交号） */
  from?: string;
  to?: string;
}

/** 对比两个版本：分支名、标签名或提交号 */
/** 提交图里多选的一个提交 */
export interface Picked {
  oid: string;
  row: number;
  summary: string;
  author: string;
  time: number;
}

/** 多选里最多放这么多个提交 */
const PICK_LIMIT = 500;

export interface CompareView {
  from: string;
  to: string;
  /** 只看 to 相对共同祖先新增的改动 */
  threeDot: boolean;
}

export interface LogLine {
  text: string;
  transient: boolean;
  /** "cmd"：执行的命令；"err"：失败信息；"ok"：完成 */
  kind?: "cmd" | "err" | "ok";
}

/** 最多保留这么多行输出日志 */
const LOG_LIMIT = 2000;

export interface FileView {
  path: string;
  mode: "history" | "blame";
  /** blame 哪个提交里的版本，空表示工作区 */
  rev: string | null;
}

export interface Toast {
  id: number;
  kind: "error" | "info";
  text: string;
}

export interface DialogSpec {
  title: string;
  message?: string;
  input?: { placeholder?: string; value: string; multiline?: boolean };
  checkbox?: { label: string; value: boolean };
  /** 单选项；input.when 设置时，只在选中这些值时显示输入框 */
  choices?: { value: string; options: { value: string; label: string; hint?: string }[] };
  inputWhen?: string[];
  confirmText?: string;
  danger?: boolean;
  resolve: (r: DialogResult | null) => void;
}

export interface DialogResult {
  input: string;
  checkbox: boolean;
  choice: string;
}

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

let tabSeq = 0;

/** 联网的操作，可能很慢，允许取消 */
const NETWORK_OPS = new Set<string>(["fetch", "pull", "push", "flow_publish"]);

/** 修改远程仓库配置的操作，完成后重新读取远程列表 */
const REMOTE_OPS = new Set<string>(["add_remote", "remove_remote", "rename_remote", "set_remote_url"]);

/** 会切换分支的操作：本地改动会被覆盖时可以先 stash */
const SWITCH_OPS = new Set<string>(["checkout", "checkout_remote", "checkout_detached", "create_branch", "checkout_pull", "bisect_start", "bisect_mark"]);
const OVERWRITE = /would be overwritten by checkout|untracked working tree files would be overwritten|commit your changes or stash them before you switch/i;
/** 拉取被本地改动挡住 */
const PULL_BLOCKED = /would be overwritten by merge|cannot pull with rebase|commit your changes or stash them before you merge|untracked working tree files would be overwritten by merge/i;
/** 分支已经在另一个工作树里检出了 */
const IN_WORKTREE = /already (?:checked out|used by worktree) at '([^']+)'/i;

/** 通知里用的操作名 */
function opName(op: Op["op"]) {
  const names: Partial<Record<Op["op"], string>> = {
    fetch: t("获取"),
    pull: t("拉取"),
    push: t("推送"),
    flow_publish: t("推送"),
    flow_finish: t("Git Flow 完成"),
    merge: t("合并"),
    rebase: t("变基"),
    rebase_interactive: t("交互式变基"),
  };
  return names[op] ?? t("操作");
}

/** 进度行冒号前面的部分就是阶段名 */
const stage = (text: string) => text.replace(/^remote:\s*/, "").split(":")[0];

export function elapsed(since: number): string {
  const s = Math.max(0, Math.round((Date.now() - since) / 1000));
  return s < 60 ? t("{0} 秒", s) : t("{0} 分 {1} 秒", Math.floor(s / 60), s % 60);
}

/** 一个标签页 = 一个打开的仓库，各自保存选中的提交、工作区状态、搜索结果等 */
export class RepoTab {
  /** 界面内部用的唯一编号（打开成功前还不知道仓库路径） */
  readonly key = ++tabSeq;
  /** 仓库工作区路径，打开成功后才有 */
  id = $state("");
  /** 用户选择的路径（可能是仓库里的子目录） */
  requestedPath: string;
  repo = $state<RepoInfo | null>(null);
  status = $state<Status | null>(null);
  stashes = $state<Stash[]>([]);
  remotes = $state<Remote[]>([]);
  selectedOid = $state<string | null>(null);
  selectedRow = $state<number | null>(null);
  /** 提交图里多选的提交（⌘/Ctrl 或 Shift 点击），按行号排（新的在前）；不到两个时为空 */
  picked = $state<Picked[]>([]);
  /** 右侧面板显示提交详情、工作区还是 stash */
  panel = $state<"commit" | "wip" | "stash">("commit");
  #diff = $state<DiffTarget | null>(null);
  /** 中间区域显示的文件差异；打开差异时关掉文件历史 */
  get diff() {
    return this.#diff;
  }
  set diff(v: DiffTarget | null) {
    this.#diff = v;
    if (v) {
      this.fileView = null;
      this.compareView = null;
      this.toolView = null;
    }
  }

  /** 打开对比视图；to 为空时等用户输入 */
  openCompare(from: string, to: string) {
    this.#diff = null;
    this.fileView = null;
    this.toolView = null;
    this.compareView = { from, to, threeDot: true };
  }

  /** 中间区域显示 reflog 或 PR 列表 */
  openTool(v: "reflog" | "pulls") {
    this.#diff = null;
    this.fileView = null;
    this.compareView = null;
    this.toolView = v;
  }
  busy = $state<string | null>(null);
  /** 当前操作开始的时间，用来显示已用时间 */
  busySince = $state(0);
  /** 当前操作是否可以取消（获取、拉取、推送等联网操作） */
  cancellable = $state(false);
  /** git 命令输出日志 */
  log = $state<LogLine[]>([]);
  /** 当前操作最新的一行进度 */
  progressLine = $state("");
  logOpen = $state(false);
  /** 中间区域显示文件历史或 blame */
  fileView = $state<FileView | null>(null);
  compareView = $state<CompareView | null>(null);
  toolView = $state<"reflog" | "pulls" | null>(null);
  /** 工作树、子模块、LFS、提交身份：打开仓库和执行操作后读一次 */
  extras = $state<RepoExtras | null>(null);
  /** 二分查错的进度，没有在进行时 active 为 false */
  bisect = $state<BisectState | null>(null);
  /** 上次获取远程更新的时间（自动获取用） */
  lastFetch = Date.now();
  autoFetching: Promise<void> | null = null;
  loading = $state(true);
  error = $state<string | null>(null);
  /** 每次重新加载历史或分支后加一，提交图据此刷新缓存 */
  graphVersion = $state(0);
  /** 让提交图滚动到某一行 */
  scrollRequest = $state<{ row: number; seq: number } | null>(null);
  search = $state({ query: "", hits: [] as number[], index: 0 });
  /** 右侧面板正在查看的 stash */
  selectedStash = $state<Stash | null>(null);
  /** Git Flow 配置（没初始化时是推荐的默认值） */
  flow = $state<FlowConfig | null>(null);

  private refreshing: Promise<void> | null = null;
  private refreshAgain = false;

  constructor(path: string) {
    this.requestedPath = path;
  }

  get name() {
    return this.repo?.name ?? this.requestedPath.split(/[\\/]/).filter(Boolean).pop() ?? t("仓库");
  }

  get changeCount() {
    const s = this.status;
    return s ? s.staged.length + s.unstaged.length + s.conflicted.length : 0;
  }

  async open() {
    this.loading = true;
    this.error = null;
    try {
      let repo = await api.openRepo(this.requestedPath);
      this.id = repo.path;
      // 恢复上次的分支筛选
      const focus = savedFocus(repo.path);
      if (focus.length) repo = await api.setFocus(repo.path, focus);
      this.repo = repo;
      this.graphVersion++;
      await Promise.all([this.refreshStatus(), this.refreshStashes(), this.refreshFlow(), this.loadRemotes(), this.loadExtras(), this.refreshBisect()]);
      if (this.extras?.lfs.used && !this.extras.lfs.installed)
        toast(t("这个仓库用了 Git LFS 管理大文件，但这台电脑没有安装 git-lfs，大文件只能看到指针。请安装 Git LFS（https://git-lfs.com）"), "error");
      // 有未提交的更改时默认显示工作区，否则显示 HEAD 提交
      if (this.changeCount > 0) {
        this.panel = "wip";
      } else if (repo.head.row !== null && repo.head.oid) {
        this.select(repo.head.row, repo.head.oid);
      }
      if (repo.head.row !== null) this.scrollTo(repo.head.row);
    } catch (e) {
      this.error = errorText(e);
    } finally {
      this.loading = false;
    }
  }

  async loadRemotes() {
    if (!this.id) return;
    this.remotes = await api.listRemotes(this.id).catch(() => []);
  }

  async loadExtras() {
    if (!this.id) return;
    try {
      this.extras = await api.repoExtras(this.id);
    } catch {
      // 读不到就保留上一次的
    }
  }

  async refreshBisect() {
    if (!this.id) return;
    this.bisect = await api.bisectState(this.id).catch(() => null);
  }

  /** 推送标签等操作默认用的远程：有 origin 用 origin，否则用第一个 */
  get defaultRemote(): Remote | undefined {
    return this.remotes.find((r) => r.name === "origin") ?? this.remotes[0];
  }

  /** 刷新分支、历史和工作区状态。多次调用会合并成一次。 */
  refresh(): Promise<void> {
    if (!this.repo) return Promise.resolve();
    if (this.refreshing) {
      this.refreshAgain = true;
      return this.refreshing;
    }
    this.refreshing = (async () => {
      try {
        do {
          this.refreshAgain = false;
          this.repo = await api.refreshRepo(this.id);
          this.graphVersion++;
          // 历史重新加载后行号可能变了，按提交哈希找回选中的行（从 reflog 打开的、不在图里的提交保持不变）
          if (this.selectedOid && this.selectedRow !== null) {
            this.selectedRow = await api.rowOf(this.id, this.selectedOid);
            if (this.selectedRow === null) this.selectedOid = null;
          }
          await this.repick();
          await Promise.all([this.refreshStatus(), this.refreshStashes(), this.refreshFlow(), this.refreshBisect()]);
        } while (this.refreshAgain);
      } catch (e) {
        toast(errorText(e), "error");
      } finally {
        this.refreshing = null;
      }
    })();
    return this.refreshing;
  }

  async refreshStatus() {
    if (!this.id) return;
    try {
      this.status = await api.status(this.id);
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  async refreshStashes() {
    try {
      this.stashes = await api.stashList(this.id);
      // stash 被删除后清掉选中状态；序号可能变化，按哈希更新
      if (this.selectedStash) {
        const cur = this.stashes.find((x) => x.oid === this.selectedStash!.oid) ?? null;
        this.selectedStash = cur;
        if (!cur && this.panel === "stash") this.panel = "commit";
      }
    } catch {
      this.stashes = [];
    }
  }

  async refreshFlow() {
    try {
      this.flow = await api.flowConfig(this.id);
    } catch {
      this.flow = null;
    }
  }

  /** 如果分支是 Git Flow 分支（例如 feature/login），返回类型和名称 */
  flowBranch(branch: string | null | undefined): { kind: FlowKind; name: string } | null {
    const f = this.flow;
    if (!f?.initialized || !branch) return null;
    for (const kind of ["feature", "bugfix", "release", "hotfix"] as FlowKind[]) {
      const prefix = f[kind];
      if (prefix && branch.startsWith(prefix) && branch.length > prefix.length) {
        return { kind, name: branch.slice(prefix.length) };
      }
    }
    return null;
  }

  /** 根据已有的版本标签推荐下一个版本号：release 升次版本号，hotfix 升修订号 */
  suggestVersion(kind: FlowKind): string {
    const prefix = this.flow?.version_tag ?? "";
    let best: number[] | null = null;
    for (const r of this.repo?.refs ?? []) {
      if (r.kind !== "tag" || !r.name.startsWith(prefix)) continue;
      const m = /^(\d+)\.(\d+)(?:\.(\d+))?$/.exec(r.name.slice(prefix.length));
      if (!m) continue;
      const v = [Number(m[1]), Number(m[2]), Number(m[3] ?? 0)];
      if (!best || v[0] > best[0] || (v[0] === best[0] && (v[1] > best[1] || (v[1] === best[1] && v[2] > best[2])))) best = v;
    }
    if (!best) return kind === "hotfix" ? "0.0.1" : "0.1.0";
    return kind === "hotfix" ? `${best[0]}.${best[1]}.${best[2] + 1}` : `${best[0]}.${best[1] + 1}.0`;
  }

  showStash(st: Stash) {
    this.picked = [];
    this.selectedStash = st;
    this.panel = "stash";
    this.selectedRow = null;
    this.selectedOid = null;
  }

  /** 只显示指定的分支（空数组 = 显示全部，["HEAD"] = 只看当前分支） */
  async setFocus(refs: string[]) {
    if (!this.id) return;
    this.busy = refs.length ? t("正在筛选分支…") : t("正在加载全部分支…");
    try {
      this.repo = await api.setFocus(this.id, refs);
      this.graphVersion++;
      saveFocus(this.id, refs);
      if (this.selectedOid) {
        this.selectedRow = await api.rowOf(this.id, this.selectedOid);
        if (this.selectedRow === null) this.selectedOid = null;
      }
      await this.repick();
      this.search.hits = [];
      const row = this.selectedRow ?? this.repo.head.row;
      if (row !== null) this.scrollTo(row);
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      this.busy = null;
    }
  }

  get focus(): string[] {
    return this.repo?.focus ?? [];
  }

  /** 执行会修改仓库的操作，完成后刷新。返回是否成功。 */
  async run(op: Op, label?: string): Promise<boolean> {
    // 自动获取还没结束时先等它，避免两个 git 进程同时改远程分支
    if (this.autoFetching && NETWORK_OPS.has(op.op)) await this.autoFetching;
    if (op.op === "fetch") this.lastFetch = Date.now();
    if (label) {
      this.busy = label;
      this.busySince = Date.now();
      this.progressLine = "";
      this.cancellable =
        NETWORK_OPS.has(op.op) || (op.op === "flow_finish" && op.options.push) || (op.op === "rename_branch" && op.remote);
      this.addLog({ text: `—— ${label.replace(/…$/, "")} · ${new Date().toLocaleTimeString()}`, transient: false, kind: "ok" });
    }
    try {
      const out = await api.gitOp(this.id, op);
      if (label) this.addLog({ text: t("✓ 完成，用时 {0}", elapsed(this.busySince)), transient: false, kind: "ok" });
      if (label && this.notifyDone()) notifyInBackground(this.name, t("{0}完成", opName(op.op)));
      if (label && out.trim()) toast(out.trim().split("\n").slice(-8).join("\n"));
      return true;
    } catch (e) {
      this.addLog({ text: `✗ ${errorText(e)}`, transient: false, kind: "err" });
      // 分支还有没合并的提交，git 拒绝普通删除：不显示原始报错，改为询问要不要强制删除
      const unmerged = op.op === "delete_branch" && !op.force && /not fully merged/i.test(errorText(e));
      // 切换分支或拉取会覆盖本地改动：不显示原始报错，改为询问要不要先 stash
      const blocked = SWITCH_OPS.has(op.op) && OVERWRITE.test(errorText(e));
      const pullBlocked = op.op === "pull" && PULL_BLOCKED.test(errorText(e));
      const inWorktree = SWITCH_OPS.has(op.op) ? IN_WORKTREE.exec(errorText(e)) : null;
      if (unmerged) {
        const tab = this;
        setTimeout(() => offerForceDelete(tab, op.name));
      } else if (blocked || pullBlocked) {
        const tab = this;
        setTimeout(() => offerStash(tab, op, label, errorText(e), pullBlocked ? "pull" : "switch"));
      } else if (inWorktree) {
        const path = inWorktree[1];
        setTimeout(() => offerOpenWorktree(path));
      } else {
        toast(errorText(e), "error");
      }
      if (label && this.notifyDone()) notifyInBackground(this.name, t("{0}失败：{1}", opName(op.op), errorText(e).split("\n")[0]));
      // 普通推送因为远程有新提交被拒绝：说明原因，可以选择强制推送
      if (op.op === "push" && !op.force && /\[rejected\]|non-fast-forward|fetch first|stale info/i.test(errorText(e))) {
        const tab = this;
        setTimeout(() => offerForcePush(tab));
      }
      await this.refresh();
      this.showConflicts();
      return false;
    } finally {
      if (label) {
        // 获取了很多新提交时重新加载历史也要一会儿，继续显示提示
        this.busy = t("正在刷新提交图…");
        this.cancellable = false;
        this.progressLine = "";
      }
      await this.refresh();
      if (REMOTE_OPS.has(op.op)) await this.loadRemotes();
      await this.loadExtras();
      if (label) this.busy = null;
    }
  }

  /** 耗时超过几秒的操作才通知，快的操作不打扰 */
  private notifyDone() {
    return notifySettings.gitOps && Date.now() - this.busySince > 4000;
  }

  /** 当前分支落后远程几个提交 */
  private behind() {
    return this.repo?.refs.find((r) => r.is_head && r.kind === "local")?.behind ?? 0;
  }

  /** 操作因为冲突停下来时，切到工作区并打开第一个冲突文件 */
  private showConflicts() {
    const c = this.status?.conflicted[0];
    if (!c) return;
    this.panel = "wip";
    this.diff = { source: "conflict", path: c.path, oldPath: null };
  }

  /** 撤销最近一次操作 */
  async undo() {
    const what = this.repo?.undo;
    if (!what || this.busy) return;
    this.busy = t("正在撤销 {0}…", what);
    this.busySince = Date.now();
    this.cancellable = false;
    try {
      toast(await api.undoLast(this.id));
      this.addLog({ text: t("↶ 撤销：{0}", what), transient: false, kind: "ok" });
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      await this.refresh();
      this.busy = null;
    }
  }

  /** 后台自动获取：不显示进度框，失败也只记在日志里 */
  autoFetch() {
    if (this.autoFetching || this.busy || !this.repo) return;
    this.lastFetch = Date.now();
    this.autoFetching = (async () => {
      this.addLog({ text: t("—— 自动获取 · {0}", new Date().toLocaleTimeString()), transient: false, kind: "ok" });
      try {
        const before = this.behind();
        await api.gitOp(this.id, { op: "fetch", quiet: true });
        await this.refresh();
        const added = this.behind() - before;
        const branch = this.repo?.head.branch;
        if (added > 0 && branch && notifySettings.remoteCommits)
          notifyInBackground(this.name, t("{0} 在远程有 {1} 个新提交，可以拉取了", branch, added));
      } catch (e) {
        this.addLog({ text: `✗ ${errorText(e)}`, transient: false, kind: "err" });
      } finally {
        this.autoFetching = null;
      }
    })();
  }

  openFile(path: string, mode: FileView["mode"], rev: string | null = null) {
    this.#diff = null;
    this.compareView = null;
    this.toolView = null;
    this.fileView = { path, mode, rev };
  }

  cancel() {
    if (!this.cancellable) return;
    this.cancellable = false;
    this.progressLine = t("正在取消…");
    api.cancelOp(this.id);
  }

  /** 收到后端转发的 git 输出 */
  onProgress(e: ProgressEvent) {
    const cmd = e.text.startsWith("$ ");
    this.addLog({ text: e.text, transient: e.transient, kind: cmd ? "cmd" : undefined });
    if (this.busy && !this.autoFetching && !cmd) this.progressLine = e.text;
  }

  private addLog(line: LogLine) {
    const last = this.log[this.log.length - 1];
    // 进度行原地更新：上一行是会被覆盖的进度，且是同一个阶段（例如 “Receiving objects: 45%”）
    if (last?.transient && !line.kind && stage(last.text) === stage(line.text)) {
      this.log[this.log.length - 1] = line;
      return;
    }
    this.log.push(line);
    if (this.log.length > LOG_LIMIT) this.log.splice(0, this.log.length - LOG_LIMIT);
  }

  select(row: number, oid: string) {
    this.picked = [];
    this.selectedRow = row;
    this.selectedOid = oid;
    this.panel = "commit";
  }

  /** ⌘/Ctrl 点击：把提交加入多选，已经选了就移出 */
  async togglePick(r: number, row: Row) {
    let list = this.picked;
    if (!list.length && this.selectedOid && this.selectedRow !== null && this.selectedRow !== r) {
      const cur = (await api.getRows(this.id, this.selectedRow, this.selectedRow + 1)).rows[0];
      list = cur ? [toPicked(this.selectedRow, cur)] : [];
    }
    const i = list.findIndex((p) => p.oid === row.oid);
    if (i >= 0) {
      list = list.filter((_, j) => j !== i);
      if (list.length === 1) return this.select(list[0].row, list[0].oid);
      if (!list.length) return;
      this.picked = list;
      if (this.selectedOid === row.oid) {
        this.selectedRow = list[list.length - 1].row;
        this.selectedOid = list[list.length - 1].oid;
      }
      return;
    }
    if (list.length >= PICK_LIMIT) return toast(t("最多选 {0} 个提交", PICK_LIMIT), "error");
    list = [...list, toPicked(r, row)].sort((a, b) => a.row - b.row);
    this.select(r, row.oid);
    if (list.length > 1) this.picked = list;
  }

  /** Shift 点击：选中从当前选中的提交到这一行之间的所有提交 */
  async pickRange(r: number, row: Row) {
    const anchor = this.selectedRow;
    if (anchor === null || anchor === r) return this.select(r, row.oid);
    const lo = Math.min(anchor, r);
    const hi = Math.min(Math.max(anchor, r), lo + PICK_LIMIT - 1);
    const rows = (await api.getRows(this.id, lo, hi + 1)).rows;
    this.picked = rows.map((x, i) => toPicked(lo + i, x));
    this.panel = "commit";
    if (Math.max(anchor, r) > hi) toast(t("最多选 {0} 个提交", PICK_LIMIT), "error");
  }

  /** 历史重新加载后按哈希找回多选的提交，找不到的去掉 */
  private async repick() {
    if (!this.picked.length) return;
    const rows = await Promise.all(this.picked.map((p) => api.rowOf(this.id, p.oid)));
    const list = this.picked.flatMap((p, i) => (rows[i] === null ? [] : [{ ...p, row: rows[i]! }])).sort((a, b) => a.row - b.row);
    this.picked = list.length > 1 ? list : [];
  }

  /** 查看一个提交；不在提交图里（row 为空）时也能在右侧看详情 */
  showCommit(oid: string, row: number | null) {
    if (row !== null) {
      this.select(row, oid);
      this.scrollTo(row);
    } else {
      this.picked = [];
      this.selectedRow = null;
      this.selectedOid = oid;
      this.panel = "commit";
    }
  }

  showWip() {
    this.picked = [];
    this.panel = "wip";
    this.selectedRow = null;
    this.selectedOid = null;
  }

  scrollTo(row: number) {
    this.scrollRequest = { row, seq: (this.scrollRequest?.seq ?? 0) + 1 };
  }

  async jumpToOid(oid: string) {
    const row = await api.rowOf(this.id, oid);
    if (row === null) return toast(t("这个提交不在当前图中"), "error");
    this.select(row, oid);
    this.scrollTo(row);
  }

  loadDiff(t: DiffTarget): Promise<FileDiff | null> {
    if (t.source === "commit") return api.commitFileDiff(this.id, t.oid!, t.path, t.oldPath);
    if (t.source === "compare") return api.compareFileDiff(this.id, t.from!, t.to!, t.path, t.oldPath);
    return api.workdirDiff(this.id, t.path, !!t.staged);
  }
}

export interface FlowDialogSpec {
  tab: RepoTab;
  mode: "init" | "start" | "finish";
  kind?: FlowKind;
  name?: string;
}

export type Theme = "dark" | "light" | "system";

export type SettingsSection = "appearance" | "git" | "identity" | "tools" | "notify" | "about";

export function openSettings(section: SettingsSection = "appearance") {
  app.settings = section;
}

function initialTheme(): Theme {
  const t = document.documentElement.dataset.theme;
  return t === "light" || t === "system" ? t : "dark";
}

export const app = $state({
  theme: initialTheme(),
  /** git 等着用户输入用户名、密码等 */
  askpass: [] as AskPassRequest[],
  /** 克隆 / 新建仓库对话框 */
  cloneDialog: null as null | { mode: "clone" | "init" },
  /** 交互式变基对话框 */
  rebaseDialog: null as null | { tab: RepoTab; from: string },
  /** 用来打开文件的编辑器命令 */
  editor: initialEditor(),
  /** 自动获取间隔（分钟），0 = 关闭 */
  autoFetch: initialAutoFetch(),
  /** 主区域显示仓库还是全局任务面板 */
  view: "repo" as "repo" | "tasks",
  /** 全局任务面板，第一次打开时加载 */
  tasks: null as Board | null,
  tabs: [] as RepoTab[],
  activeKey: 0,
  toasts: [] as Toast[],
  dialog: null as DialogSpec | null,
  /** 提交身份设置对话框 */
  identityDialog: null as RepoTab | null,
  /** 设置窗口打开时显示的分类 */
  settings: null as SettingsSection | null,
  flowDialog: null as FlowDialogSpec | null,
});

export async function showTasks() {
  app.view = "tasks";
  await preloadTasks();
}

/** 启动时在后台加载任务，标签栏上能显示未完成的任务数 */
export async function preloadTasks() {
  if (!app.tasks) {
    try {
      app.tasks = await loadTasks();
      if (autoArchive(app.tasks)) saveTasksSoon();
    } catch (e) {
      toast(errorText(e), "error");
    }
  }
}

let tasksTimer = 0;
/** 修改任务后调用：合并 0.4 秒内的多次修改，一起保存 */
export function saveTasksSoon() {
  clearTimeout(tasksTimer);
  tasksTimer = window.setTimeout(() => {
    if (app.tasks) saveTasks($state.snapshot(app.tasks) as Board).catch((e) => toast(t("任务保存失败：{0}", errorText(e)), "error"));
  }, 400);
}

function initialEditor(): string {
  try {
    return localStorage.getItem("gittree.editor") || "code";
  } catch {
    return "code";
  }
}

export function setEditor(cmd: string) {
  if (!cmd.trim()) return;
  app.editor = cmd.trim();
  try {
    localStorage.setItem("gittree.editor", app.editor);
  } catch {
    // 忽略
  }
}

/** 用外部程序打开仓库或其中的文件 */
export function openIn(tab: RepoTab, target: OpenTarget, path: string | null = null) {
  api.openIn(tab.id, target, path, app.editor).catch((e) => toast(errorText(e), "error"));
}

/** 正在拖动的分支（dragover 时拿不到 dataTransfer 里的数据，所以单独记下来） */
export const drag = { ref: null as RefInfo | null };

function initialAutoFetch(): number {
  try {
    const v = localStorage.getItem("gittree.autoFetch");
    return v === null ? 10 : Number(v) || 0;
  } catch {
    return 10;
  }
}

export function setAutoFetch(minutes: number) {
  app.autoFetch = minutes;
  try {
    localStorage.setItem("gittree.autoFetch", String(minutes));
  } catch {
    // 忽略
  }
}

/** 每隔一会儿检查一次，到时间的仓库在后台获取；一次只获取一个，避免同时占用网络 */
export function startAutoFetch() {
  const timer = setInterval(() => {
    const ms = app.autoFetch * 60_000;
    if (!ms || app.tabs.some((t) => t.autoFetching)) return;
    const due = app.tabs
      .filter((t) => t.repo && !t.busy && Date.now() - t.lastFetch >= ms)
      .sort((a, b) => a.lastFetch - b.lastFetch)[0];
    due?.autoFetch();
  }, 20_000);
  return () => clearInterval(timer);
}

export function setTheme(t: Theme) {
  app.theme = t;
  document.documentElement.dataset.theme = t;
  try {
    localStorage.setItem("gittree.theme", t);
  } catch {
    // 忽略
  }
}

/** 打开 Git Flow 对话框（初始化 / 开始 / 完成） */
export function openFlow(spec: FlowDialogSpec) {
  app.flowDialog = spec;
}

export function activeTab(): RepoTab | undefined {
  return app.tabs.find((t) => t.key === app.activeKey);
}

let toastSeq = 0;
export function toast(text: string, kind: Toast["kind"] = "info") {
  const id = ++toastSeq;
  app.toasts.push({ id, kind, text });
  setTimeout(() => (app.toasts = app.toasts.filter((t) => t.id !== id)), kind === "error" ? 9000 : 3500);
}

export function dialog(spec: Omit<DialogSpec, "resolve">): Promise<DialogResult | null> {
  return new Promise((resolve) => {
    app.dialog = { ...spec, resolve };
  });
}

/** 强制推送当前分支，先确认 */
export async function forcePush(tab: RepoTab) {
  const branch = tab.repo?.head.branch;
  if (!branch) return toast(t("没有检出分支，无法推送"), "error");
  const ok = await confirm(
    t("强制推送"),
    t(
      "用本地的 {0} 覆盖远程分支？\n远程上有、本地没有的提交会被丢掉。\n如果在你上次获取之后别人又推送了新提交，这次推送会失败，不会覆盖别人的提交（--force-with-lease）。",
      branch,
    ),
    t("强制推送"),
    true,
  );
  if (ok) await tab.run({ op: "push", force: true }, t("正在强制推送…"));
}

async function offerStash(tab: RepoTab, op: Op, label: string | undefined, error: string, kind: "switch" | "pull") {
  // git 的报错里，会被覆盖的文件每行一个，以制表符开头
  const files = error
    .split("\n")
    .filter((l) => /^\t/.test(l))
    .map((l) => l.trim());
  const list = files.slice(0, 8).join("\n") + (files.length > 8 ? `\n${t("…还有 {0} 个文件", files.length - 8)}` : "");
  const pull = kind === "pull";
  const d = await dialog({
    title: t("本地改动会被覆盖"),
    message: pull
      ? t("拉取下来的提交会改动下面这些文件，而它们还有没提交的改动。可以先把改动 stash 起来，拉取后再恢复。\n\n{0}", list || "—")
      : t("切换会覆盖下面这些文件里还没提交的改动，可以先把改动 stash 起来再切换。\n\n{0}", list || "—"),
    checkbox: { label: pull ? t("拉取后恢复这些改动（stash pop）") : t("切换后恢复这些改动（stash pop）"), value: true },
    confirmText: pull ? t("Stash 并拉取") : t("Stash 并切换"),
  });
  if (d) await tab.run({ op: "with_stash", inner: op, pop: d.checkbox }, label ?? (pull ? t("正在拉取…") : t("正在切换分支…")));
}

async function offerOpenWorktree(path: string) {
  const ok = await confirm(
    t("分支在另一个工作树里"),
    t("这个分支已经在工作树 {0} 里检出了，同一个分支不能同时在两个地方检出。\n要打开那个工作树吗？", path),
    t("打开工作树"),
  );
  if (ok) await openRepo(path);
}

async function offerForceDelete(tab: RepoTab, name: string) {
  const ok = await confirm(
    t("分支还没有合并"),
    t("{0} 上有还没合并到当前分支（或它跟踪的远程分支）的提交，删除后这些提交就找不到了。\n确定要强制删除吗？删除后可以用「撤销」（{1}Z）找回。", name, isMac ? "⌘" : "Ctrl+"),
    t("强制删除"),
    true,
  );
  if (ok) await tab.run({ op: "delete_branch", name, force: true });
}

async function offerForcePush(tab: RepoTab) {
  const ok = await confirm(
    t("推送被拒绝"),
    t(
      "远程分支上有你本地没有的提交。\n· 如果是别人推送了新提交：请先拉取，再推送。\n· 如果是你刚变基或修改过已经推送的提交，确定要用本地覆盖远程：可以强制推送。",
    ),
    t("强制推送…"),
    true,
  );
  if (ok) await forcePush(tab);
}

/** 当前分支对应的远程仓库：跟踪的远程，没有就用默认的 */
function remoteOf(tab: RepoTab, branch: RefInfo | undefined): Remote | undefined {
  const name = branch?.upstream?.split("/")[0];
  return tab.remotes.find((r) => r.name === name) ?? tab.defaultRemote;
}

/** 在浏览器里为分支创建 PR / MR（默认当前分支） */
export async function createPull(tab: RepoTab, branchName?: string) {
  const refs = tab.repo?.refs ?? [];
  const name = branchName ?? tab.repo?.head.branch;
  const branch = refs.find((r) => r.kind === "local" && r.name === name);
  if (!branch) return toast(t("请先检出一个分支"), "error");
  const remote = remoteOf(tab, branch);
  if (!remote) return toast(t("这个仓库还没有配置远程仓库"), "error");
  const info = await api.forgeInfo(remote.url).catch((e) => (toast(errorText(e), "error"), null));
  if (!info) return;
  // 公司自己部署的平台认不出来：到 PR 页面里选
  if (!info.kind) return tab.openTool("pulls");
  let source = branch.upstream ? branch.upstream.slice(remote.name.length + 1) : branch.name;
  if (!branch.upstream || branch.ahead > 0) {
    const ok = await confirm(
      branch.upstream ? t("还有没推送的提交") : t("分支还没有推送"),
      branch.upstream
        ? t("{0} 有 {1} 个提交还没推送，PR 里不会包含它们。先推送吗？", branch.name, branch.ahead)
        : t("{0} 还没有推送到远程仓库，要先推送才能创建 PR。现在推送吗？", branch.name),
      t("推送"),
    );
    if (ok) {
      if (!branch.is_head) return toast(t("请先切换到 {0} 再推送", branch.name), "error");
      if (!(await tab.run({ op: "push", force: false }, t("正在推送…")))) return;
      source = tab.repo?.refs.find((r) => r.kind === "local" && r.name === branch.name)?.upstream?.slice(remote.name.length + 1) ?? branch.name;
    } else if (!branch.upstream) return;
  }
  const remoteBranches = refs.filter((r) => r.kind === "remote" && r.name.startsWith(remote.name + "/")).map((r) => r.name.slice(remote.name.length + 1));
  const guess = ["main", "master", "develop", "dev"].find((b) => remoteBranches.includes(b) && b !== source) ?? "main";
  const d = await dialog({
    title: info.kind === "gitlab" ? t("创建 Merge Request") : t("创建 Pull Request"),
    message: t("把 {0} 合并到哪个分支？会在浏览器里打开 {1} 的创建页面。", source, info.project.host),
    input: { placeholder: t("目标分支"), value: guess },
    confirmText: t("在浏览器中创建"),
  });
  if (!d?.input.trim()) return;
  try {
    await api.openUrl(await api.forgeNewPullUrl(remote.url, source, d.input.trim()));
  } catch (e) {
    toast(errorText(e), "error");
  }
}

// ---------- 工作树、子模块 ----------

/** 新工作树默认放在仓库旁边：<仓库名>-<分支名> */
function defaultWorktreePath(tab: RepoTab, branch: string) {
  const sep = tab.id.includes("\\") ? "\\" : "/";
  return `${tab.id.replace(/[\\/]+$/, "")}-${branch.replace(/[\\/:*?"<>|\s]+/g, "-")}`.replace(/\//g, sep);
}

/** 在新工作树里检出分支；branch 为空时新建分支 */
export async function addWorktree(tab: RepoTab, branch?: string) {
  let name = branch;
  let isNew = false;
  if (!name) {
    const d = await dialog({
      title: t("新建工作树"),
      message: t("把一个分支检出到另一个文件夹，两边可以同时修改、编译，不用来回切换和 stash。\n输入已有的分支名，或者一个新的分支名（从当前 HEAD 创建）。"),
      input: { placeholder: t("分支名称"), value: "" },
      confirmText: t("下一步"),
    });
    name = d?.input.trim();
    if (!name) return;
    isNew = !tab.repo?.refs.some((r) => r.kind === "local" && r.name === name);
  }
  const d = await dialog({
    title: isNew ? t("新建分支 {0} 并放到新工作树", name) : t("在新工作树中检出 {0}", name),
    message: t("工作树的文件夹（不能已经存在）"),
    input: { placeholder: t("文件夹路径"), value: defaultWorktreePath(tab, name) },
    checkbox: { label: t("创建后在新标签页打开"), value: true },
    confirmText: t("创建"),
  });
  const path = d?.input.trim();
  if (!d || !path) return;
  const ok = await tab.run({ op: "add_worktree", path, branch: name, new_branch: isNew, start: isNew ? "HEAD" : null }, t("正在创建工作树…"));
  if (ok && d.checkbox) await openRepo(path);
}

export async function addSubmodule(tab: RepoTab) {
  const u = await dialog({
    title: t("添加子模块"),
    input: { placeholder: "git@gitee.com:user/lib.git", value: "" },
    confirmText: t("下一步"),
  });
  const url = u?.input.trim();
  if (!url) return;
  const guess = url.replace(/\.git$/, "").split(/[/:]/).pop() ?? "lib";
  const p = await dialog({ title: t("放到哪个目录"), message: t("仓库里的相对路径"), input: { placeholder: "libs/name", value: guess }, confirmText: t("添加") });
  if (p?.input.trim()) await tab.run({ op: "submodule_add", url, path: p.input.trim() }, t("正在添加子模块…"));
}

/** 开始二分查错：当前 HEAD 有问题，问一个没问题的版本 */
export async function startBisect(tab: RepoTab) {
  const tags = (tab.repo?.refs ?? []).filter((r) => r.kind === "tag").sort((a, b) => b.name.localeCompare(a.name, undefined, { numeric: true }));
  const d = await dialog({
    title: t("开始二分查错"),
    message: t(
      "当前检出的 {0} 有问题。输入一个确定没问题的版本（标签、分支或提交号），git 会在两者之间一步步帮你找出第一个引入问题的提交。\n也可以在提交图里右键一个提交，选「从这里开始二分查错」。",
      tab.repo?.head.branch ?? "HEAD",
    ),
    input: { placeholder: t("没问题的版本，例如 v1.2.0"), value: tags[0]?.name ?? "" },
    confirmText: t("开始"),
  });
  if (d?.input.trim()) await tab.run({ op: "bisect_start", bad: "HEAD", good: d.input.trim() }, t("正在开始二分查错…"));
}

/** 让 Git LFS 管理一类文件，pattern 为空时询问 */
export async function trackWithLfs(tab: RepoTab, pattern: string | null) {
  const d = await dialog({
    title: t("用 Git LFS 管理"),
    message: t("符合规则的文件以后会存到 LFS 服务器上，仓库里只保存指针。会修改 .gitattributes，记得提交它。\n已经提交过的文件不受影响。"),
    input: { placeholder: "*.psd", value: pattern ?? "" },
    confirmText: t("确定"),
  });
  if (d?.input.trim()) await tab.run({ op: "lfs_track", pattern: d.input.trim() });
}

/** 把未跟踪的文件加入 .gitignore */
export async function ignorePatterns(tab: RepoTab, patterns: string[]) {
  if (await tab.run({ op: "ignore", patterns }))
    toast(patterns.length === 1 ? t("已加入 .gitignore：{0}", patterns[0]) : t("已把 {0} 个文件加入 .gitignore", patterns.length));
}

export function copyText(text: string) {
  navigator.clipboard.writeText(text).then(
    () => toast(t("已复制：{0}", text)),
    () => toast(t("复制失败"), "error"),
  );
}

/** 复制远程仓库地址（去掉地址里的密码 / token） */
export function copyRemoteUrl(r: Remote) {
  copyText(shareUrl(r.url));
}

/** 在浏览器里打开远程仓库的网页 */
export function openRemoteWeb(r: Remote) {
  const url = webUrl(r.url);
  if (!url) return toast(t("认不出 {0} 的网页地址", r.url), "error");
  api.openUrl(url).catch((e) => toast(errorText(e), "error"));
}

/** 添加远程仓库：先问名称，再问地址 */
export async function addRemote(tab: RepoTab) {
  const name = await dialog({
    title: t("添加远程仓库"),
    message: t("名称，例如 origin、upstream"),
    input: { placeholder: t("远程仓库名称"), value: tab.remotes.length ? "upstream" : "origin" },
    confirmText: t("下一步"),
  });
  if (!name?.input.trim()) return;
  const url = await dialog({
    title: t("添加远程仓库 {0}", name.input.trim()),
    input: { placeholder: "git@gitee.com:user/repo.git", value: "" },
    checkbox: { label: t("添加后立即获取"), value: true },
    confirmText: t("添加"),
  });
  if (!url?.input.trim()) return;
  if (await tab.run({ op: "add_remote", name: name.input.trim(), url: url.input.trim() }) && url.checkbox)
    await tab.run({ op: "fetch" }, t("正在获取远程更新…"));
}

export async function confirm(title: string, message: string, confirmText = t("确定"), danger = false) {
  return (await dialog({ title, message, confirmText, danger })) !== null;
}

/** 合并 source：选择合并方式后执行；into 指定时先切换到 into 再合并 */
export async function mergeBranch(tab: RepoTab, source: string, into?: string) {
  const current = tab.repo?.head.branch ?? "HEAD";
  const last = prefs.mergeMode();
  const d = await dialog({
    title: t("合并"),
    message: into ? t("先切换到 {0}，再把 {1} 合并进去", into, source) : t("将 {0} 合并到当前分支 {1}", source, current),
    choices: {
      value: last,
      options: [
        { value: "auto", label: t("自动"), hint: t("能快进就直接快进，否则创建合并提交（git 默认）") },
        { value: "no_ff", label: t("总是创建合并提交"), hint: t("即使能快进也保留一个合并提交，历史里看得出分支（--no-ff）") },
        { value: "ff_only", label: t("只允许快进"), hint: t("两边已经分叉时不合并（--ff-only）") },
        { value: "squash", label: t("压缩成一个提交"), hint: t("把 {0} 的所有改动合成一个新提交，不保留它的提交历史（--squash）", source) },
      ],
    },
    input: { placeholder: t("提交说明"), value: `Merge branch '${source}' (squashed)`, multiline: true },
    inputWhen: ["squash"],
    confirmText: t("合并"),
  });
  if (!d) return;
  const mode = d.choice as MergeMode;
  prefs.setMergeMode(mode);
  const message = mode === "squash" ? d.input.trim() || null : null;
  await tab.run(
    into ? { op: "merge_into", branch: into, source, mode, message } : { op: "merge", target: source, mode, message },
    t("正在合并…"),
  );
}

// ---------- 最近打开的仓库和标签页记忆（只存在本机） ----------

function readJson<T>(key: string, fallback: T): T {
  try {
    return JSON.parse(localStorage.getItem(key) ?? "") ?? fallback;
  } catch {
    return fallback;
  }
}
function writeJson(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // 浏览器存储不可用时忽略
  }
}

const RECENT_KEY = "gittree.recent";
const FOCUS_KEY = "gittree.focus";

function savedFocus(repo: string): string[] {
  return readJson<Record<string, string[]>>(FOCUS_KEY, {})[repo] ?? [];
}
function saveFocus(repo: string, refs: string[]) {
  const all = readJson<Record<string, string[]>>(FOCUS_KEY, {});
  if (refs.length) all[repo] = refs;
  else delete all[repo];
  writeJson(FOCUS_KEY, all);
}
const TABS_KEY = "gittree.tabs";

export function recentRepos(): string[] {
  return readJson<string[]>(RECENT_KEY, []);
}
function rememberRepo(path: string) {
  writeJson(RECENT_KEY, [path, ...recentRepos().filter((p) => p !== path)].slice(0, 15));
}
export function forgetRepo(path: string) {
  writeJson(RECENT_KEY, recentRepos().filter((p) => p !== path));
}

/** 记住打开了哪些标签页，下次启动时恢复 */
function saveTabs() {
  const active = activeTab();
  writeJson(TABS_KEY, {
    paths: app.tabs.filter((t) => t.id).map((t) => t.id),
    active: active?.id ?? null,
  });
}

export async function restoreTabs(extra: string | null) {
  const saved = readJson<{ paths: string[]; active: string | null }>(TABS_KEY, { paths: [], active: null });
  for (const p of saved.paths) addTab(p, false);
  if (extra) {
    await openRepo(extra);
  } else {
    const active = app.tabs.find((t) => t.requestedPath === saved.active) ?? app.tabs[0];
    if (active) app.activeKey = active.key;
  }
  await Promise.all(app.tabs.map((t) => (t.repo || t.error ? null : openTab(t))));
}

function addTab(path: string, activate: boolean): RepoTab {
  const tab = new RepoTab(path);
  app.tabs.push(tab);
  if (activate) app.activeKey = tab.key;
  return tab;
}

async function openTab(tab: RepoTab) {
  await tab.open();
  if (!tab.id) return;
  // 同一个仓库已经在别的标签页打开了：保留原来的
  const dup = app.tabs.find((t) => t !== tab && t.id === tab.id);
  if (dup) {
    if (app.activeKey === tab.key) app.activeKey = dup.key;
    app.tabs = app.tabs.filter((t) => t !== tab);
  } else {
    rememberRepo(tab.id);
  }
  saveTabs();
}

/** 在新标签页打开仓库；已经打开的直接切换过去 */
export async function openRepo(path: string) {
  app.view = "repo";
  const existing = app.tabs.find((t) => t.id === path || t.requestedPath === path);
  if (existing) {
    app.activeKey = existing.key;
    if (existing.error) await openTab(existing);
    saveTabs();
    return;
  }
  await openTab(addTab(path, true));
}

export function activate(tab: RepoTab) {
  app.activeKey = tab.key;
  app.view = "repo";
  saveTabs();
  // 切过来时顺便刷新一下，可能在别处改过
  tab.refresh();
}

export function closeTab(tab: RepoTab) {
  const i = app.tabs.indexOf(tab);
  app.tabs = app.tabs.filter((t) => t !== tab);
  if (tab.id) api.closeRepo(tab.id).catch(() => {});
  if (app.activeKey === tab.key) {
    const next = app.tabs[Math.min(i, app.tabs.length - 1)];
    app.activeKey = next?.key ?? 0;
  }
  saveTabs();
}

export function tabById(id: string) {
  return app.tabs.find((t) => t.id === id);
}

function toPicked(row: number, r: Row): Picked {
  return { oid: r.oid, row, summary: r.summary, author: r.author, time: r.time };
}
