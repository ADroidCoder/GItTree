import { invoke } from "@tauri-apps/api/core";

export type RefKind = "local" | "remote" | "tag";

export interface RefInfo {
  full: string;
  name: string;
  kind: RefKind;
  oid: string;
  row: number | null;
  is_head: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
}

export interface HeadInfo {
  branch: string | null;
  oid: string | null;
  row: number | null;
}

export interface Timings {
  read_ms: number;
  sort_ms: number;
  graph_ms: number;
  refs_ms: number;
  from_commit_graph: number;
  from_objects: number;
}

export interface RepoInfo {
  path: string;
  name: string;
  head: HeadInfo;
  refs: RefInfo[];
  total: number;
  max_lanes: number;
  timings: Timings;
  reloaded: boolean;
  /** 只显示哪些分支：空 = 全部，["HEAD"] = 只看当前分支 */
  focus: string[];
  /** 下一步撤销会撤销什么，没有可撤销的操作时为 null */
  undo: string | null;
}

export interface Row {
  oid: string;
  summary: string;
  author: string;
  email: string;
  time: number;
  col: number;
  color: number;
  parents: number;
  refs: number[];
}

/** [编号, 子行, 父行, 子列, 父列, 泳道, 颜色] */
export type Edge = [number, number, number, number, number, number, number];

export interface RowsOut {
  start: number;
  rows: Row[];
  edges: Edge[];
}

export interface FileChange {
  path: string;
  old_path: string | null;
  status: string;
  additions: number;
  deletions: number;
  binary: boolean;
}

export interface Person {
  name: string;
  email: string;
  time: number;
  offset: number;
}

export interface CommitDetails {
  oid: string;
  parents: string[];
  author: Person;
  committer: Person;
  message: string;
  files: FileChange[];
}

export interface DiffLine {
  kind: "+" | "-" | " ";
  old: number | null;
  new: number | null;
  text: string;
}

export interface Hunk {
  header: string;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  old_path: string | null;
  binary: boolean;
  hunks: Hunk[];
  truncated: boolean;
}

export interface StatusEntry {
  path: string;
  old_path: string | null;
  index: string;
  worktree: string;
  conflicted: boolean;
}

export interface Status {
  staged: StatusEntry[];
  unstaged: StatusEntry[];
  conflicted: StatusEntry[];
  in_progress: string | null;
}

export interface Stash {
  index: number;
  name: string;
  message: string;
  oid: string;
}

export type FlowKind = "feature" | "bugfix" | "release" | "hotfix";

export interface FlowConfig {
  master: string;
  develop: string;
  feature: string;
  bugfix: string;
  release: string;
  hotfix: string;
  version_tag: string;
  initialized: boolean;
}

export interface FinishOptions {
  keep_branch: boolean;
  tag_message: string | null;
  push: boolean;
}

export type Op =
  | { op: "stage"; paths: string[] }
  | { op: "stage_all" }
  | { op: "unstage"; paths: string[] }
  | { op: "unstage_all" }
  | { op: "discard"; tracked: string[]; untracked: string[] }
  | { op: "stage_hunk"; path: string; header: string; staged: boolean }
  | { op: "discard_hunk"; path: string; header: string }
  | { op: "commit"; message: string; amend: boolean }
  | { op: "checkout"; name: string }
  | { op: "checkout_remote"; remote: string; local: string }
  | { op: "checkout_detached"; oid: string }
  | { op: "create_branch"; name: string; start: string; checkout: boolean }
  | { op: "delete_branch"; name: string; force: boolean }
  | { op: "delete_remote_branch"; name: string }
  | { op: "merge"; target: string }
  | { op: "rebase"; onto: string }
  | { op: "sequencer"; kind: string; action: "abort" | "continue" }
  | { op: "cherry_pick"; oid: string }
  | { op: "revert"; oid: string }
  | { op: "reset"; oid: string; mode: "soft" | "mixed" | "hard" }
  | { op: "create_tag"; name: string; oid: string; message: string | null }
  | { op: "delete_tag"; name: string }
  | { op: "fetch"; quiet?: boolean }
  | { op: "pull" }
  | { op: "push"; force: boolean }
  | { op: "stash_push"; message: string | null; paths?: string[] }
  | { op: "stash"; action: "pop" | "apply" | "drop"; index: number }
  | { op: "flow_init"; config: FlowConfig }
  | { op: "flow_start"; kind: FlowKind; name: string; base: string | null }
  | { op: "flow_finish"; kind: FlowKind; name: string; options: FinishOptions }
  | { op: "flow_publish"; kind: FlowKind; name: string }
  | { op: "resolve_conflict"; path: string; choices: Choice[] }
  | { op: "resolve_side"; path: string; theirs: boolean }
  | { op: "merge_into"; branch: string; source: string }
  | { op: "rebase_branch"; branch: string; onto: string }
  | { op: "rebase_interactive"; from: string; steps: RebaseStep[] };

export interface PlanCommit {
  oid: string;
  parents: number;
  author: string;
  time: number;
  summary: string;
  message: string;
}

export interface RebaseStep {
  oid: string;
  action: "pick" | "squash" | "drop";
  /** 新的提交说明，null 表示不改 */
  message: string | null;
}

export type OpenTarget = "folder" | "terminal" | "editor";

/** 冲突文件解析后的一段：普通内容，或者一处冲突 */
export type Chunk =
  | { kind: "common"; text: string }
  | { kind: "conflict"; ours: string; base: string | null; theirs: string; ours_label: string; theirs_label: string };

export interface ConflictFile {
  path: string;
  chunks: Chunk[];
  conflicts: number;
  /** UTF-8 文本才能手动编辑 */
  utf8: boolean;
  binary: boolean;
  ours_exists: boolean;
  theirs_exists: boolean;
}

/** 对一处冲突的选择 */
export type Choice =
  | { pick: "ours" | "theirs" | "base" | "ours_theirs" | "theirs_ours" }
  | { pick: "text"; text: string };

export interface HistoryEntry {
  oid: string;
  author: string;
  email: string;
  time: number;
  summary: string;
  /** 这个提交里文件的路径（重命名前不一样） */
  path: string;
  old_path: string | null;
  status: string;
}

export interface BlameCommit {
  oid: string;
  author: string;
  time: number;
  summary: string;
  uncommitted: boolean;
}

export interface Blame {
  commits: BlameCommit[];
  lines: string[];
  /** 每一行来自 commits 里的哪一个 */
  line_commit: number[];
}

/** 所有和仓库相关的接口都带上 repo（工作区路径），支持同时打开多个仓库 */
export const api = {
  openRepo: (path: string) => invoke<RepoInfo>("open_repo", { path }),
  closeRepo: (repo: string) => invoke<void>("close_repo", { repo }),
  refreshRepo: (repo: string) => invoke<RepoInfo>("refresh_repo", { repo }),
  setFocus: (repo: string, refs: string[]) => invoke<RepoInfo>("set_focus", { repo, refs }),
  getRows: (repo: string, start: number, end: number) => invoke<RowsOut>("get_rows", { repo, start, end }),
  search: (repo: string, query: string) => invoke<number[]>("search", { repo, query }),
  rowOf: (repo: string, oid: string) => invoke<number | null>("row_of", { repo, oid }),
  commitDetails: (repo: string, oid: string) => invoke<CommitDetails>("commit_details", { repo, oid }),
  commitFileDiff: (repo: string, oid: string, path: string, oldPath: string | null) =>
    invoke<FileDiff | null>("commit_file_diff", { repo, oid, path, oldPath }),
  status: (repo: string) => invoke<Status>("status", { repo }),
  workdirDiff: (repo: string, path: string, staged: boolean) =>
    invoke<FileDiff | null>("workdir_diff", { repo, path, staged }),
  stashList: (repo: string) => invoke<Stash[]>("stash_list", { repo }),
  lastCommitMessage: (repo: string) => invoke<string>("last_commit_message", { repo }),
  gitOp: (repo: string, op: Op) => invoke<string>("git_op", { repo, op }),
  cancelOp: (repo: string) => invoke<void>("cancel_op", { repo }),
  cloneRepo: (url: string, parent: string, name: string) => invoke<string>("clone_repo", { url, parent, name }),
  initRepo: (path: string) => invoke<void>("init_repo", { path }),
  rebasePlan: (repo: string, from: string) => invoke<PlanCommit[]>("rebase_plan", { repo, from }),
  openIn: (repo: string, target: OpenTarget, path: string | null, editor: string | null) =>
    invoke<void>("open_in", { repo, target, path, editor }),
  askpassReply: (id: number, answer: string | null) => invoke<void>("askpass_reply", { id, answer }),
  undoLast: (repo: string) => invoke<string>("undo_last", { repo }),
  conflictFile: (repo: string, path: string) => invoke<ConflictFile>("conflict_file", { repo, path }),
  fileHistory: (repo: string, path: string) => invoke<HistoryEntry[]>("file_history", { repo, path }),
  blame: (repo: string, path: string, rev: string | null) => invoke<Blame>("blame", { repo, path, rev }),
  flowConfig: (repo: string) => invoke<FlowConfig>("flow_config", { repo }),
  initialRepo: () => invoke<string | null>("initial_repo"),
};

/** 后端转发的 git 输出（"git-progress" 事件） */
export interface ProgressEvent {
  repo: string;
  text: string;
  /** 会被下一行原地覆盖的进度行（git 用 \r 刷新百分比） */
  transient: boolean;
}

/** git / ssh 需要用户输入（"askpass" 事件） */
export interface AskPassRequest {
  id: number;
  prompt: string;
}
