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

export interface Remote {
  name: string;
  url: string;
  /** 和 url 不同时才有 */
  push_url: string | null;
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
  /** 文件太大，没有逐行对比：改动前后的大小（字节）。full 传 true 才对比 */
  too_large: [number, number] | null;
}

export interface CompareCommit {
  oid: string;
  summary: string;
  author: string;
  time: number;
}

export interface Compare {
  from: string;
  to: string;
  base: string | null;
  files: FileChange[];
  /** to 有、from 没有的提交（最多 500 个），新的在前 */
  commits: CompareCommit[];
  ahead: number;
  behind: number;
}

export interface ReflogEntry {
  oid: string;
  time: number;
  /** commit、checkout、reset、rebase、merge、pull 等 */
  action: string;
  message: string;
  summary: string;
  author: string;
}

export interface Worktree {
  path: string;
  head: string;
  branch: string | null;
  main: boolean;
  current: boolean;
  locked: boolean;
  prunable: boolean;
}

export interface Submodule {
  name: string;
  path: string;
  url: string;
  oid: string;
  state: "ok" | "uninitialized" | "modified" | "conflict";
  describe: string;
}

export interface LfsInfo {
  installed: boolean;
  used: boolean;
  patterns: string[];
}

/** 只下载了一部分的仓库 */
export interface PartialInfo {
  /** 浅克隆：更早的历史没有下载 */
  shallow: boolean;
  /** 文件内容用到时才下载 */
  lazy: boolean;
  /** 开了部分检出时检出的目录 */
  sparse: string[] | null;
}

export interface CloneOptions {
  depth: number | null;
  sparse: string[] | null;
}

export interface Identity {
  name: string | null;
  email: string | null;
  /** local、global、system、command */
  scope: string | null;
  origin: string | null;
}

export interface IdentityProfile {
  id: string;
  label: string;
  name: string;
  email: string;
  signing_key: string | null;
  dirs: string[];
  file: string;
}

export interface RepoExtras {
  worktrees: Worktree[];
  submodules: Submodule[];
  lfs: LfsInfo;
  identity: Identity;
  partial: PartialInfo;
}

export interface BisectState {
  active: boolean;
  bad: string | null;
  good: string[];
  skipped: string[];
  current: string | null;
  remaining: number | null;
  found: string | null;
}

export interface Signature {
  /** G 有效，U 有效但密钥未受信任，B 无效，X/Y 过期，R 已吊销，E 无法验证，N 没有签名 */
  status: string;
  signer: string;
  key: string;
  format: "gpg" | "ssh";
}

export interface ImageSide {
  data: string | null;
  size: number;
  lfs: boolean;
}

export interface ImagePair {
  old: ImageSide | null;
  new: ImageSide | null;
}

export type ForgeKind = "github" | "gitlab" | "gitee" | "gitea";

export interface ForgeInfo {
  project: { host: string; path: string; web: string };
  kind: ForgeKind | null;
  has_token: boolean;
  pulls_web: string | null;
  token_page: string | null;
}

export interface PullRequest {
  number: number;
  title: string;
  author: string;
  source: string;
  target: string;
  /** 来自 fork 仓库 */
  fork: boolean;
  /** fork 仓库的 owner/repo，GitLab 没有 */
  source_repo: string | null;
  draft: boolean;
  updated: string;
  url: string;
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
  | { op: "reword"; message: string }
  | { op: "checkout"; name: string }
  | { op: "checkout_remote"; remote: string; local: string }
  | { op: "checkout_detached"; oid: string }
  | { op: "create_branch"; name: string; start: string; checkout: boolean }
  | { op: "delete_branch"; name: string; force: boolean }
  | { op: "delete_remote_branch"; name: string }
  | { op: "rename_branch"; old: string; new: string; remote: boolean }
  | { op: "with_stash"; inner: Op; pop: boolean }
  | { op: "stage_lines"; path: string; header: string; staged: boolean; lines: number[] }
  | { op: "discard_lines"; path: string; header: string; lines: number[] }
  | { op: "add_remote"; name: string; url: string }
  | { op: "remove_remote"; name: string }
  | { op: "rename_remote"; old: string; new: string }
  | { op: "set_remote_url"; name: string; url: string }
  | { op: "push_tag"; remote: string; tag: string | null }
  | { op: "delete_remote_tag"; remote: string; tag: string }
  | { op: "merge"; target: string; mode?: MergeMode; message?: string | null }
  | { op: "rebase"; onto: string }
  | { op: "sequencer"; kind: string; action: "abort" | "continue" | "skip" }
  | { op: "cherry_pick"; oids: string[] }
  | { op: "revert"; oids: string[] }
  | { op: "reset"; oid: string; mode: "soft" | "mixed" | "hard" }
  | { op: "create_tag"; name: string; oid: string; message: string | null }
  | { op: "delete_tag"; name: string }
  | { op: "fetch"; quiet?: boolean; overwrite_tags?: boolean }
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
  | { op: "merge_into"; branch: string; source: string; mode?: MergeMode; message?: string | null }
  | { op: "rebase_branch"; branch: string; onto: string }
  | { op: "rebase_interactive"; from: string; steps: RebaseStep[] }
  | { op: "ignore"; patterns: string[] }
  | { op: "submodule_update"; path: string | null }
  | { op: "submodule_update_remote"; path: string }
  | { op: "submodule_sync"; path: string | null }
  | { op: "submodule_add"; url: string; path: string }
  | { op: "add_worktree"; path: string; branch: string; new_branch: boolean; start: string | null }
  | { op: "remove_worktree"; path: string; force: boolean }
  | { op: "prune_worktrees" }
  | { op: "bisect_start"; bad: string; good: string }
  | { op: "bisect_mark"; verdict: "good" | "bad" | "skip"; oid: string | null }
  | { op: "bisect_reset" }
  | { op: "lfs_pull" }
  | { op: "lfs_fetch" }
  | { op: "lfs_track"; pattern: string }
  | { op: "lfs_untrack"; pattern: string }
  | { op: "set_local_identity"; name: string; email: string }
  | { op: "clear_local_identity" }
  | { op: "checkout_pull"; remote: string; remote_ref: string; local: string; number: number }
  | { op: "apply_patch"; files: string[]; text: string | null }
  | { op: "deepen"; more: number | null }
  | { op: "sparse_set"; dirs: string[] }
  | { op: "sparse_disable" };

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
  /** 文件太大没有逐处解析（字节数），只能整个选一边 */
  too_large: number | null;
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
  /** 日期搜索时 query 是「起..止」的秒数，可以只填一端 */
  search: (repo: string, query: string, kind: SearchKind = "text") => invoke<number[]>("search", { repo, query, kind }),
  rowOf: (repo: string, oid: string) => invoke<number | null>("row_of", { repo, oid }),
  commitDetails: (repo: string, oid: string) => invoke<CommitDetails>("commit_details", { repo, oid }),
  commitFileDiff: (repo: string, oid: string, path: string, oldPath: string | null, full = false) =>
    invoke<FileDiff | null>("commit_file_diff", { repo, oid, path, oldPath, full }),
  status: (repo: string) => invoke<Status>("status", { repo }),
  workdirDiff: (repo: string, path: string, staged: boolean, full = false) =>
    invoke<FileDiff | null>("workdir_diff", { repo, path, staged, full }),
  stashList: (repo: string) => invoke<Stash[]>("stash_list", { repo }),
  lastCommitMessage: (repo: string) => invoke<string>("last_commit_message", { repo }),
  gitOp: (repo: string, op: Op) => invoke<string>("git_op", { repo, op }),
  cancelOp: (repo: string) => invoke<void>("cancel_op", { repo }),
  cloneRepo: (url: string, parent: string, name: string, options: CloneOptions) => invoke<string>("clone_repo", { url, parent, name, options }),
  initRepo: (path: string) => invoke<void>("init_repo", { path }),
  rebasePlan: (repo: string, from: string) => invoke<PlanCommit[]>("rebase_plan", { repo, from }),
  listRemotes: (repo: string) => invoke<Remote[]>("list_remotes", { repo }),
  compare: (repo: string, from: string, to: string, threeDot: boolean) => invoke<Compare>("compare", { repo, from, to, threeDot }),
  compareFileDiff: (repo: string, from: string, to: string, path: string, oldPath: string | null, full = false) =>
    invoke<FileDiff | null>("compare_file_diff", { repo, from, to, path, oldPath, full }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  reflog: (repo: string, target: string | null) => invoke<(ReflogEntry & { row: number | null })[]>("reflog", { repo, target }),
  repoExtras: (repo: string) => invoke<RepoExtras>("repo_extras", { repo }),
  /** outDir 为空时返回一段补丁文本，否则返回写出的文件 */
  exportPatches: (repo: string, oids: string[], outDir: string | null) => invoke<string[]>("export_patches", { repo, oids, outDir }),
  sparseSubdirs: (repo: string, parent: string) => invoke<string[]>("sparse_subdirs", { repo, parent }),
  commitPushed: (repo: string, oid: string) => invoke<boolean>("commit_pushed", { repo, oid }),
  bisectState: (repo: string) => invoke<BisectState>("bisect_state", { repo }),
  commitSignature: (repo: string, oid: string) => invoke<Signature | null>("commit_signature", { repo, oid }),
  imagePair: (
    repo: string,
    t: { source: string; oid?: string; from?: string; to?: string; staged?: boolean; path: string; oldPath: string | null },
  ) =>
    invoke<ImagePair>("image_pair", {
      repo,
      source: t.source,
      oid: t.oid ?? null,
      from: t.from ?? null,
      to: t.to ?? null,
      staged: !!t.staged,
      path: t.path,
      oldPath: t.oldPath,
    }),
  identityProfiles: () => invoke<IdentityProfile[]>("identity_profiles"),
  identitySaveProfile: (id: string | null, label: string, name: string, email: string, signingKey: string | null) =>
    invoke<string>("identity_save_profile", { id, label, name, email, signingKey }),
  identityDeleteProfile: (id: string) => invoke<void>("identity_delete_profile", { id }),
  identitySetDirs: (id: string, dirs: string[]) => invoke<void>("identity_set_dirs", { id, dirs }),
  globalIdentity: () => invoke<Identity>("global_identity"),
  setGlobalIdentity: (name: string, email: string) => invoke<void>("set_global_identity", { name, email }),
  forgeInfo: (url: string) => invoke<ForgeInfo>("forge_info", { url }),
  forgeNewPullUrl: (url: string, source: string, target: string) => invoke<string>("forge_new_pull_url", { url, source, target }),
  forgeConfigure: (host: string, kind: ForgeKind | null, token: string | null) => invoke<void>("forge_configure", { host, kind, token }),
  pullRequests: (url: string) => invoke<{ kind: ForgeKind; pulls: PullRequest[] }>("pull_requests", { url }),
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

/** 合并方式 */
export type MergeMode = "auto" | "no_ff" | "ff_only" | "squash";

/** 搜提交的方式：说明/作者/哈希、改过的文件路径、改动的代码、日期 */
export type SearchKind = "text" | "path" | "code" | "date";
