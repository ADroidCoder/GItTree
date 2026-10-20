import { loadTasks, saveTasks, type Board } from "./board";
import { api, type FileDiff, type FlowConfig, type FlowKind, type Op, type OpenTarget, type ProgressEvent, type RefInfo, type RepoInfo, type Stash, type Status } from "./api";

export interface DiffTarget {
  /** commit：某个提交里的文件；workdir：工作区文件；conflict：解决冲突 */
  source: "commit" | "workdir" | "conflict";
  path: string;
  oldPath: string | null;
  oid?: string;
  staged?: boolean;
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
  confirmText?: string;
  danger?: boolean;
  resolve: (r: { input: string; checkbox: boolean } | null) => void;
}

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

let tabSeq = 0;

/** 联网的操作，可能很慢，允许取消 */
const NETWORK_OPS = new Set<string>(["fetch", "pull", "push", "flow_publish"]);

/** 进度行冒号前面的部分就是阶段名 */
const stage = (text: string) => text.replace(/^remote:\s*/, "").split(":")[0];

export function elapsed(since: number): string {
  const s = Math.max(0, Math.round((Date.now() - since) / 1000));
  return s < 60 ? `${s} 秒` : `${Math.floor(s / 60)} 分 ${s % 60} 秒`;
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
  selectedOid = $state<string | null>(null);
  selectedRow = $state<number | null>(null);
  /** 右侧面板显示提交详情、工作区还是 stash */
  panel = $state<"commit" | "wip" | "stash">("commit");
  #diff = $state<DiffTarget | null>(null);
  /** 中间区域显示的文件差异；打开差异时关掉文件历史 */
  get diff() {
    return this.#diff;
  }
  set diff(v: DiffTarget | null) {
    this.#diff = v;
    if (v) this.fileView = null;
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
    return this.repo?.name ?? this.requestedPath.split(/[\\/]/).filter(Boolean).pop() ?? "仓库";
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
      await Promise.all([this.refreshStatus(), this.refreshStashes(), this.refreshFlow()]);
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
          // 历史重新加载后行号可能变了，按提交哈希找回选中的行
          if (this.selectedOid) {
            this.selectedRow = await api.rowOf(this.id, this.selectedOid);
            if (this.selectedRow === null) this.selectedOid = null;
          }
          await Promise.all([this.refreshStatus(), this.refreshStashes(), this.refreshFlow()]);
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
    this.selectedStash = st;
    this.panel = "stash";
    this.selectedRow = null;
    this.selectedOid = null;
  }

  /** 只显示指定的分支（空数组 = 显示全部，["HEAD"] = 只看当前分支） */
  async setFocus(refs: string[]) {
    if (!this.id) return;
    this.busy = refs.length ? "正在筛选分支…" : "正在加载全部分支…";
    try {
      this.repo = await api.setFocus(this.id, refs);
      this.graphVersion++;
      saveFocus(this.id, refs);
      if (this.selectedOid) {
        this.selectedRow = await api.rowOf(this.id, this.selectedOid);
        if (this.selectedRow === null) this.selectedOid = null;
      }
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
      this.cancellable = NETWORK_OPS.has(op.op) || (op.op === "flow_finish" && op.options.push);
      this.addLog({ text: `—— ${label.replace(/…$/, "")} · ${new Date().toLocaleTimeString()}`, transient: false, kind: "ok" });
    }
    try {
      const out = await api.gitOp(this.id, op);
      if (label) this.addLog({ text: `✓ 完成，用时 ${elapsed(this.busySince)}`, transient: false, kind: "ok" });
      if (label && out.trim()) toast(out.trim().split("\n").slice(-8).join("\n"));
      return true;
    } catch (e) {
      this.addLog({ text: `✗ ${errorText(e)}`, transient: false, kind: "err" });
      toast(errorText(e), "error");
      await this.refresh();
      this.showConflicts();
      return false;
    } finally {
      if (label) {
        // 获取了很多新提交时重新加载历史也要一会儿，继续显示提示
        this.busy = "正在刷新提交图…";
        this.cancellable = false;
        this.progressLine = "";
      }
      await this.refresh();
      if (label) this.busy = null;
    }
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
    this.busy = `正在撤销 ${what}…`;
    this.busySince = Date.now();
    this.cancellable = false;
    try {
      toast(await api.undoLast(this.id));
      this.addLog({ text: `↶ 撤销：${what}`, transient: false, kind: "ok" });
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
      this.addLog({ text: `—— 自动获取 · ${new Date().toLocaleTimeString()}`, transient: false, kind: "ok" });
      try {
        await api.gitOp(this.id, { op: "fetch" });
        await this.refresh();
      } catch (e) {
        this.addLog({ text: `✗ ${errorText(e)}`, transient: false, kind: "err" });
      } finally {
        this.autoFetching = null;
      }
    })();
  }

  openFile(path: string, mode: FileView["mode"], rev: string | null = null) {
    this.#diff = null;
    this.fileView = { path, mode, rev };
  }

  cancel() {
    if (!this.cancellable) return;
    this.cancellable = false;
    this.progressLine = "正在取消…";
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
    this.selectedRow = row;
    this.selectedOid = oid;
    this.panel = "commit";
  }

  showWip() {
    this.panel = "wip";
    this.selectedRow = null;
    this.selectedOid = null;
  }

  scrollTo(row: number) {
    this.scrollRequest = { row, seq: (this.scrollRequest?.seq ?? 0) + 1 };
  }

  async jumpToOid(oid: string) {
    const row = await api.rowOf(this.id, oid);
    if (row === null) return toast("这个提交不在当前图中", "error");
    this.select(row, oid);
    this.scrollTo(row);
  }

  loadDiff(t: DiffTarget): Promise<FileDiff | null> {
    if (t.source === "commit") return api.commitFileDiff(this.id, t.oid!, t.path, t.oldPath);
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

function initialTheme(): Theme {
  const t = document.documentElement.dataset.theme;
  return t === "light" || t === "system" ? t : "dark";
}

export const app = $state({
  theme: initialTheme(),
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
    if (app.tasks) saveTasks($state.snapshot(app.tasks) as Board).catch((e) => toast(`任务保存失败：${errorText(e)}`, "error"));
  }, 400);
}

function initialEditor(): string {
  try {
    return localStorage.getItem("gittree.editor") || "code";
  } catch {
    return "code";
  }
}

export async function chooseEditor() {
  const r = await dialog({
    title: "编辑器命令",
    message: "用来打开仓库和文件的命令，例如 code（VS Code）、cursor、idea、subl、webstorm",
    input: { value: app.editor, placeholder: "code" },
    confirmText: "保存",
  });
  if (!r?.input.trim()) return;
  app.editor = r.input.trim();
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

export function dialog(spec: Omit<DialogSpec, "resolve">): Promise<{ input: string; checkbox: boolean } | null> {
  return new Promise((resolve) => {
    app.dialog = { ...spec, resolve };
  });
}

export async function confirm(title: string, message: string, confirmText = "确定", danger = false) {
  return (await dialog({ title, message, confirmText, danger })) !== null;
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
