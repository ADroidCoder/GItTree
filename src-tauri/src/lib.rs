mod askpass;
mod background;

use gittree_core::tr;
use gittree_core::gix::{self, ObjectId};
use gittree_core::log::{self, Error, HeadInfo, Log, RefInfo, Timings};
use gittree_core::flow::{self, FinishOptions, FlowConfig};
use gittree_core::{conflict, diff, git2, history, ops, rebase, undo};
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

/// 一个打开的仓库（界面上的一个标签页）
struct Session {
    workdir: PathBuf,
    git_dir: PathBuf,
    repo: gix::ThreadSafeRepository,
    log: Log,
    /// 只显示哪些分支（空 = 全部，["HEAD"] = 只看当前分支）
    focus: Vec<String>,
    /// 每次重新加载历史都会变化，后台任务据此判断结果是否过期
    generation: u64,
    /// 可以撤销的操作，最新的在最后
    undo: Vec<undo::Entry>,
    _watcher: Option<notify::RecommendedWatcher>,
}

/// 最多记住这么多步撤销
const UNDO_LIMIT: usize = 50;

type SessionRef = Arc<Mutex<Session>>;

/// 每个仓库单独加锁，一个标签页在加载或执行操作时不会卡住其他标签页
#[derive(Default)]
struct AppState {
    sessions: Mutex<HashMap<String, SessionRef>>,
    /// 正在执行的 git 操作，用来取消
    running: Mutex<HashMap<String, Arc<ops::Progress>>>,
    generation: AtomicU64,
}

impl AppState {
    fn get(&self, repo: &str) -> Result<SessionRef, Error> {
        let map = self.sessions.lock().map_err(|_| tr!("内部状态错误", "Internal state error"))?;
        map.get(repo).cloned().ok_or_else(|| tr!("仓库没有打开或已经关闭", "The repository isn't open").into())
    }

    fn next_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }
}

type CmdResult<T> = Result<T, String>;

/// 在后台线程执行耗时操作，不阻塞界面
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, Error> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

fn lock(s: &SessionRef) -> Result<std::sync::MutexGuard<'_, Session>, Error> {
    s.lock().map_err(|_| tr!("内部状态错误", "Internal state error").into())
}

#[derive(Serialize)]
struct RepoInfo {
    /// 工作区路径，同时作为界面上标签页的标识
    path: String,
    name: String,
    head: HeadInfo,
    refs: Vec<RefInfo>,
    total: usize,
    max_lanes: u16,
    timings: Timings,
    /// 是否重新加载了提交历史（false 表示只刷新了分支信息）
    reloaded: bool,
    focus: Vec<String>,
    /// 下一步撤销会撤销什么，没有可撤销的操作时为空
    undo: Option<String>,
}

fn repo_info(s: &Session, reloaded: bool) -> RepoInfo {
    RepoInfo {
        path: s.workdir.to_string_lossy().into_owned(),
        name: s.workdir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        head: s.log.head.clone(),
        refs: s.log.refs.clone(),
        total: s.log.len(),
        max_lanes: s.log.graph.max_lanes,
        timings: s.log.timings.clone(),
        reloaded,
        focus: s.focus.clone(),
        undo: s.undo.last().map(|e| e.label.clone()),
    }
}

/// 后台多线程读取所有提交的作者和说明
fn spawn_meta_loader(session: SessionRef, generation: u64) {
    std::thread::spawn(move || {
        let job = {
            let Ok(s) = session.lock() else { return };
            if s.generation != generation || s.log.meta_missing == 0 {
                return;
            }
            (s.repo.clone(), s.log.oids.clone(), s.log.missing_meta_rows())
        };
        let (repo, oids, rows) = job;
        let metas = log::read_meta_parallel(&repo, &oids, &rows);
        if let Ok(mut s) = session.lock()
            && s.generation == generation
        {
            s.log.install_meta(metas);
        }
    });
}

/// 仓库还没有 commit-graph（或很旧）时在后台生成一个，下次打开会快很多。
/// 这和 git gc 自动做的事情一样，只是写一个缓存文件。
fn maybe_write_commit_graph(workdir: PathBuf, from_objects: u32) {
    if from_objects < 2000 {
        return;
    }
    std::thread::spawn(move || {
        let _ = ops::git(&workdir, &["commit-graph", "write", "--reachable", "--no-progress"]);
    });
}

/// 监听 .git 目录（分支、HEAD、暂存区变化）和工作区文件变化，合并后通知界面。
/// 事件内容是仓库标识，界面据此刷新对应的标签页。
fn watch(app: AppHandle, id: String, workdir: &Path, git_dir: &Path) -> Option<notify::RecommendedWatcher> {
    let repo_dirty = Arc::new(AtomicBool::new(false));
    let work_dirty = Arc::new(AtomicBool::new(false));
    let git_dir_owned = git_dir.to_path_buf();
    let (rd, wd) = (repo_dirty.clone(), work_dirty.clone());
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        for p in &ev.paths {
            if p.starts_with(&git_dir_owned) {
                let rel = p.strip_prefix(&git_dir_owned).unwrap_or(p);
                // 对象文件和锁文件的变化不用管
                if rel.starts_with("objects") || rel.extension().is_some_and(|e| e == "lock") {
                    continue;
                }
                rd.store(true, Ordering::Relaxed);
            } else {
                wd.store(true, Ordering::Relaxed);
            }
        }
    })
    .ok()?;
    watcher.watch(workdir, RecursiveMode::Recursive).ok()?;
    // .git 不在工作区里时（例如 git worktree）单独监听
    if !git_dir.starts_with(workdir) {
        let _ = watcher.watch(git_dir, RecursiveMode::Recursive);
    }
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(400));
            if Arc::strong_count(&repo_dirty) == 1 {
                return; // watcher 已经被释放（标签页关闭了）
            }
            if repo_dirty.swap(false, Ordering::Relaxed) {
                work_dirty.store(false, Ordering::Relaxed);
                let _ = app.emit("repo-changed", &id);
            } else if work_dirty.swap(false, Ordering::Relaxed) {
                let _ = app.emit("workdir-changed", &id);
            }
        }
    });
    Some(watcher)
}

/// Windows 上路径可能带 \\?\ 前缀，git 命令不认识，这里去掉
fn dunce(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => p,
    }
}

/// 打开仓库。path 可以是仓库里的任意子目录；同一个仓库重复打开会重新加载。
/// 界面语言，决定后端提示信息用中文还是英文
#[tauri::command]
fn set_lang(en: bool) {
    gittree_core::set_english(en);
}

#[tauri::command]
async fn open_repo(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<RepoInfo> {
    let generation = state.next_generation();
    let (session, info) = blocking(move || {
        let repo = gix::discover(&path).map_err(|e| tr!("不是 Git 仓库：{e}", "Not a Git repository: {e}"))?;
        let workdir = dunce(repo.workdir().ok_or_else(|| tr!("暂不支持裸仓库", "Bare repositories aren't supported yet"))?.to_path_buf());
        let git_dir = dunce(repo.git_dir().to_path_buf());
        let mut repo = repo;
        repo.object_cache_size_if_unset(32 * 1024 * 1024);
        let log = Log::load_repo(&repo, &[])?;
        maybe_write_commit_graph(workdir.clone(), log.timings.from_objects);
        let id = workdir.to_string_lossy().into_owned();
        let watcher = watch(app, id, &workdir, &git_dir);
        let s = Session { workdir, git_dir, repo: repo.into_sync(), log, focus: Vec::new(), generation, undo: Vec::new(), _watcher: watcher };
        let info = repo_info(&s, true);
        Ok((Arc::new(Mutex::new(s)), info))
    })
    .await?;
    state
        .sessions
        .lock()
        .map_err(|_| tr!("内部状态错误", "Internal state error"))?
        .insert(info.path.clone(), session.clone());
    spawn_meta_loader(session, generation);
    Ok(info)
}

/// 关闭标签页时释放内存和文件监听
#[tauri::command]
fn close_repo(state: State<'_, AppState>, repo: String) {
    if let Ok(mut map) = state.sessions.lock() {
        map.remove(&repo);
    }
}

/// 刷新：如果分支指向的提交都没变，只刷新分支信息；否则重新加载历史
#[tauri::command]
async fn refresh_repo(state: State<'_, AppState>, repo: String) -> CmdResult<RepoInfo> {
    let session = state.get(&repo).map_err(|e| e.to_string())?;
    let next_gen = state.next_generation();
    let session2 = session.clone();
    let (info, reloaded) = blocking(move || {
        let mut s = lock(&session2)?;
        let repo = s.repo.to_thread_local();
        let tips = log::read_tips(&repo, &s.focus)?;
        if tips == s.log.tips {
            s.log.reload_refs(&repo)?;
            return Ok((repo_info(&s, false), false));
        }
        s.log = Log::load_repo(&repo, &s.focus)?;
        s.generation = next_gen;
        Ok((repo_info(&s, true), true))
    })
    .await?;
    if reloaded {
        spawn_meta_loader(session, next_gen);
    }
    Ok(info)
}

/// 设置只显示哪些分支并重新加载历史。refs 为空表示显示全部，["HEAD"] 表示只看当前分支
#[tauri::command]
async fn set_focus(state: State<'_, AppState>, repo: String, refs: Vec<String>) -> CmdResult<RepoInfo> {
    let session = state.get(&repo).map_err(|e| e.to_string())?;
    let next_gen = state.next_generation();
    let session2 = session.clone();
    let info = blocking(move || {
        let mut s = lock(&session2)?;
        let repo = s.repo.to_thread_local();
        s.log = Log::load_repo(&repo, &refs)?;
        s.focus = refs;
        s.generation = next_gen;
        Ok(repo_info(&s, true))
    })
    .await?;
    spawn_meta_loader(session, next_gen);
    Ok(info)
}

#[derive(Serialize)]
struct RowOut {
    oid: String,
    summary: String,
    author: String,
    email: String,
    time: i64,
    col: u16,
    color: u8,
    parents: u8,
    refs: Vec<u32>,
}

#[derive(Serialize)]
struct RowsOut {
    start: u32,
    rows: Vec<RowOut>,
    /// [编号, 子行, 父行, 子列, 父列, 泳道, 颜色]
    edges: Vec<[u32; 7]>,
}

#[tauri::command]
async fn get_rows(state: State<'_, AppState>, repo: String, start: u32, end: u32) -> CmdResult<RowsOut> {
    let session = state.get(&repo).map_err(|e| e.to_string())?;
    blocking(move || {
        let mut s = lock(&session)?;
        let n = s.log.len() as u32;
        let (start, end) = (start.min(n), end.min(n));
        let local = s.repo.to_thread_local();
        s.log.ensure_meta(&local, start as usize..end as usize);
        let log = &s.log;
        let rows = (start..end)
            .map(|r| {
                let i = r as usize;
                let m = &log.meta[i];
                let (author, email) = log.author(i).map(|(a, e)| (a.to_string(), e.to_string())).unwrap_or_default();
                RowOut {
                    oid: log.oids[i].to_string(),
                    summary: m.summary.to_string(),
                    author,
                    email,
                    time: m.author_time,
                    col: log.graph.node_col[i],
                    color: log.graph.node_color[i],
                    parents: log.parents_of(i).len() as u8,
                    refs: log.refs_at.get(&r).cloned().unwrap_or_default(),
                }
            })
            .collect();
        let edges = log
            .graph
            .edges_in(start, end)
            .into_iter()
            .map(|(id, e)| [id, e.child_row, e.parent_row, e.child_col as u32, e.parent_col as u32, e.lane as u32, e.color as u32])
            .collect();
        Ok(RowsOut { start, rows, edges })
    })
    .await
}

#[tauri::command]
async fn search(state: State<'_, AppState>, repo: String, query: String) -> CmdResult<Vec<u32>> {
    let session = state.get(&repo).map_err(|e| e.to_string())?;
    blocking(move || {
        // 后台还没读完说明时，先把剩下的读完再搜（读的时候不占着锁）
        let job = {
            let s = lock(&session)?;
            (s.log.meta_missing > 0).then(|| (s.repo.clone(), s.log.oids.clone(), s.log.missing_meta_rows()))
        };
        if let Some((repo, oids, rows)) = job {
            let metas = log::read_meta_parallel(&repo, &oids, &rows);
            lock(&session)?.log.install_meta(metas);
        }
        Ok(lock(&session)?.log.search(&query, 5000))
    })
    .await
}

#[tauri::command]
async fn row_of(state: State<'_, AppState>, repo: String, oid: String) -> CmdResult<Option<u32>> {
    let session = state.get(&repo).map_err(|e| e.to_string())?;
    blocking(move || {
        let s = lock(&session)?;
        Ok(ObjectId::from_hex(oid.as_bytes()).ok().and_then(|id| s.log.row_of.get(&id).copied()))
    })
    .await
}

fn dirs_of(state: &AppState, repo: &str) -> CmdResult<(PathBuf, PathBuf)> {
    let session = state.get(repo).map_err(|e| e.to_string())?;
    let s = session.lock().map_err(|_| tr!("内部状态错误", "Internal state error"))?;
    Ok((s.workdir.clone(), s.git_dir.clone()))
}

#[tauri::command]
async fn commit_details(state: State<'_, AppState>, repo: String, oid: String) -> CmdResult<diff::CommitDetails> {
    let (dir, _) = dirs_of(&state, &repo)?;
    blocking(move || diff::commit_details(&git2::Repository::open(dir)?, &oid)).await
}

#[tauri::command]
async fn commit_file_diff(
    state: State<'_, AppState>,
    repo: String,
    oid: String,
    path: String,
    old_path: Option<String>,
) -> CmdResult<Option<diff::FileDiff>> {
    let (dir, _) = dirs_of(&state, &repo)?;
    blocking(move || diff::commit_file_diff(&git2::Repository::open(dir)?, &oid, &path, old_path.as_deref())).await
}

#[tauri::command]
async fn status(state: State<'_, AppState>, repo: String) -> CmdResult<ops::Status> {
    let (dir, git_dir) = dirs_of(&state, &repo)?;
    blocking(move || ops::status(&dir, &git_dir)).await
}

#[tauri::command]
async fn workdir_diff(state: State<'_, AppState>, repo: String, path: String, staged: bool) -> CmdResult<Option<diff::FileDiff>> {
    let (dir, _) = dirs_of(&state, &repo)?;
    blocking(move || diff::workdir_file_diff(&git2::Repository::open(dir)?, &path, staged)).await
}

#[tauri::command]
async fn stash_list(state: State<'_, AppState>, repo: String) -> CmdResult<Vec<ops::Stash>> {
    let (dir, _) = dirs_of(&state, &repo)?;
    blocking(move || ops::stash_list(&dir)).await
}

#[tauri::command]
async fn last_commit_message(state: State<'_, AppState>, repo: String) -> CmdResult<String> {
    let (dir, _) = dirs_of(&state, &repo)?;
    blocking(move || ops::git(&dir, &["log", "-1", "--format=%B"])).await
}

#[tauri::command]
async fn flow_config(state: State<'_, AppState>, repo: String) -> CmdResult<FlowConfig> {
    let (dir, _) = dirs_of(&state, &repo)?;
    blocking(move || Ok(flow::config(&dir))).await
}

/// 所有会修改仓库的操作
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Op {
    Stage { paths: Vec<String> },
    StageAll,
    Unstage { paths: Vec<String> },
    UnstageAll,
    Discard { tracked: Vec<String>, untracked: Vec<String> },
    /// 暂存（staged = false）或取消暂存（staged = true）一个代码块
    StageHunk { path: String, header: String, staged: bool },
    DiscardHunk { path: String, header: String },
    Commit { message: String, amend: bool },
    Checkout { name: String },
    CheckoutRemote { remote: String, local: String },
    CheckoutDetached { oid: String },
    CreateBranch { name: String, start: String, checkout: bool },
    DeleteBranch { name: String, force: bool },
    DeleteRemoteBranch { name: String },
    RenameBranch { old: String, new: String, remote: bool },
    Merge { target: String },
    Rebase { onto: String },
    Sequencer { kind: String, action: String },
    CherryPick { oid: String },
    Revert { oid: String },
    Reset { oid: String, mode: String },
    CreateTag { name: String, oid: String, message: Option<String> },
    DeleteTag { name: String },
    Fetch {
        /// 后台自动获取：需要登录时不弹窗
        #[serde(default)]
        quiet: bool,
    },
    Pull,
    Push { force: bool },
    StashPush {
        message: Option<String>,
        #[serde(default)]
        paths: Vec<String>,
    },
    Stash { action: String, index: u32 },
    FlowInit { config: FlowConfig },
    FlowStart { kind: flow::Kind, name: String, base: Option<String> },
    FlowFinish { kind: flow::Kind, name: String, options: FinishOptions },
    FlowPublish { kind: flow::Kind, name: String },
    /// 按每一处冲突的选择解决冲突文件
    ResolveConflict { path: String, choices: Vec<conflict::Choice> },
    /// 整个文件使用当前（theirs = false）或传入（theirs = true）的版本
    ResolveSide { path: String, theirs: bool },
    /// 拖拽：切换到 branch 并合并 source
    MergeInto { branch: String, source: String },
    /// 拖拽：把 branch 变基到 onto
    RebaseBranch { branch: String, onto: String },
    RebaseInteractive { from: String, steps: Vec<rebase::Step> },
}

impl Op {
    /// 可以撤销的操作：返回撤销记录里显示的名称，以及怎样移回当前分支
    fn undo_label(&self) -> Option<(String, undo::Mode)> {
        use undo::Mode::{Keep, Soft};
        Some(match self {
            Op::Commit { message, amend } => {
                let first = message.lines().next().unwrap_or("").trim();
                (
                    match (amend, gittree_core::english()) {
                        (true, true) => format!("amend “{first}”"),
                        (false, true) => format!("commit “{first}”"),
                        (true, false) => format!("修改提交 “{first}”"),
                        (false, false) => format!("提交 “{first}”"),
                    },
                    Soft,
                )
            }
            Op::Checkout { name } => (tr!("切换到 {name}", "checkout {name}"), Keep),
            Op::CheckoutRemote { local, .. } => (tr!("检出 {local}", "checkout {local}"), Keep),
            Op::CheckoutDetached { oid } => (tr!("检出提交 {}", "checkout {}", &oid[..oid.len().min(7)]), Keep),
            Op::CreateBranch { name, .. } => (tr!("新建分支 {name}", "create branch {name}"), Keep),
            Op::DeleteBranch { name, .. } => (tr!("删除分支 {name}", "delete branch {name}"), Keep),
            Op::Merge { target } => (tr!("合并 {target}", "merge {target}"), Keep),
            Op::Rebase { onto } => (tr!("变基到 {onto}", "rebase onto {onto}"), Keep),
            Op::CherryPick { oid } => (tr!("拣选 {}", "cherry-pick {}", &oid[..oid.len().min(7)]), Keep),
            Op::Revert { oid } => (tr!("还原 {}", "revert {}", &oid[..oid.len().min(7)]), Keep),
            Op::Reset { oid, mode } => (tr!("重置到 {}（{mode}）", "reset to {} ({mode})", &oid[..oid.len().min(7)]), Keep),
            Op::CreateTag { name, .. } => (tr!("新建标签 {name}", "create tag {name}"), Keep),
            Op::DeleteTag { name } => (tr!("删除标签 {name}", "delete tag {name}"), Keep),
            Op::Pull => (tr!("拉取", "pull"), Keep),
            Op::FlowStart { kind, name, .. } => (tr!("开始 {kind:?} {name}", "start {kind:?} {name}"), Keep),
            Op::FlowFinish { kind, name, .. } => (tr!("完成 {kind:?} {name}", "finish {kind:?} {name}"), Keep),
            Op::MergeInto { branch, source } => (tr!("把 {source} 合并到 {branch}", "merge {source} into {branch}"), Keep),
            Op::RebaseBranch { branch, onto } => (tr!("把 {branch} 变基到 {onto}", "rebase {branch} onto {onto}"), Keep),
            Op::RebaseInteractive { steps, .. } => (tr!("交互式变基（{} 个提交）", "interactive rebase ({} commits)", steps.len()), Keep),
            _ => return None,
        })
    }
}

fn hunk(dir: &Path, path: &str, header: &str, staged: bool) -> Result<Vec<u8>, Error> {
    diff::hunk_patch(&git2::Repository::open(dir)?, path, staged, header)?
        .ok_or_else(|| tr!("这个代码块已经变化了，请刷新后重试", "This hunk has changed. Refresh and try again").into())
}

#[derive(Clone, Serialize)]
struct ProgressEvent<'a> {
    repo: &'a str,
    text: String,
    transient: bool,
}

/// 创建一个把 git 输出转发给界面的进度上下文，并登记到 running 里以便取消。
/// key 是仓库标识（克隆时是 "clone"）
fn start_progress(app: AppHandle, state: &AppState, key: &str) -> Arc<ops::Progress> {
    // \r 刷新的百分比行很密，限制在每秒 10 次左右
    let id = key.to_string();
    let last = Mutex::new(std::time::Instant::now() - Duration::from_secs(1));
    let progress = ops::Progress::new(move |line| {
        if line.transient {
            let mut t = last.lock().unwrap();
            if t.elapsed() < Duration::from_millis(100) {
                return;
            }
            *t = std::time::Instant::now();
        }
        let _ = app.emit("git-progress", ProgressEvent { repo: &id, text: line.text, transient: line.transient });
    });
    if let Ok(mut m) = state.running.lock() {
        m.insert(key.to_string(), progress.clone());
    }
    progress
}

fn finish_progress(state: &AppState, key: &str, progress: &Arc<ops::Progress>) {
    if let Ok(mut m) = state.running.lock()
        && m.get(key).is_some_and(|x| Arc::ptr_eq(x, progress))
    {
        m.remove(key);
    }
}

#[tauri::command]
async fn git_op(app: AppHandle, state: State<'_, AppState>, repo: String, op: Op) -> CmdResult<String> {
    let (dir, git_dir) = dirs_of(&state, &repo)?;
    let progress = start_progress(app, &state, &repo);
    let p = progress.clone();
    let session = state.get(&repo).map_err(|e| e.to_string())?;
    let result = blocking(move || {
        // 会移动分支的操作前后各拍一次快照，用来撤销
        let undoable = op.undo_label();
        let before = match &undoable {
            Some(_) => Some(undo::snapshot(&dir)?),
            None => None,
        };
        let out = ops::with_progress(p, || run_op(&dir, &git_dir, op))?;
        if let (Some((label, mode)), Some(before)) = (undoable, before)
            && let Some(entry) = undo::record(label, mode, before, undo::snapshot(&dir)?)
        {
            let mut s = lock(&session)?;
            s.undo.push(entry);
            if s.undo.len() > UNDO_LIMIT {
                s.undo.remove(0);
            }
        }
        Ok(out)
    })
    .await;
    finish_progress(&state, &repo, &progress);
    result
}

/// 克隆仓库，进度事件的 repo 是 "clone"
#[tauri::command]
async fn clone_repo(app: AppHandle, state: State<'_, AppState>, url: String, parent: String, name: String) -> CmdResult<String> {
    let progress = start_progress(app, &state, "clone");
    let p = progress.clone();
    let result = blocking(move || {
        ops::with_progress(p, || ops::clone(Path::new(&parent), url.trim(), name.trim()))
            .map(|d| d.to_string_lossy().into_owned())
    })
    .await;
    finish_progress(&state, "clone", &progress);
    result
}

#[tauri::command]
async fn init_repo(path: String) -> CmdResult<()> {
    blocking(move || ops::init(Path::new(&path))).await
}

#[tauri::command]
async fn rebase_plan(state: State<'_, AppState>, repo: String, from: String) -> CmdResult<Vec<rebase::PlanCommit>> {
    let (dir, _) = dirs_of(&state, &repo)?;
    blocking(move || rebase::plan(&dir, &from)).await
}

/// 用外部程序打开：folder = 文件管理器，terminal = 终端，editor = 编辑器命令（例如 code、cursor、idea）。
/// path 是仓库里的相对路径，为空时打开整个仓库
#[tauri::command]
async fn open_in(state: State<'_, AppState>, repo: String, target: String, path: Option<String>, editor: Option<String>) -> CmdResult<()> {
    let (dir, _) = dirs_of(&state, &repo)?;
    blocking(move || {
        let file = path.as_deref().filter(|p| !p.is_empty()).map(|p| dir.join(p));
        let spawn = |cmd: &str, args: &[&std::ffi::OsStr]| -> Result<(), Error> {
            let mut c = std::process::Command::new(cmd);
            c.args(args).current_dir(&dir);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                c.creation_flags(0x0800_0000);
            }
            c.spawn().map_err(|e| tr!("无法运行 {cmd}：{e}", "Couldn't run {cmd}: {e}"))?;
            Ok(())
        };
        let d = dir.as_os_str();
        match target.as_str() {
            "folder" => {
                #[cfg(target_os = "macos")]
                return match &file {
                    Some(f) => spawn("open", &["-R".as_ref(), f.as_os_str()]),
                    None => spawn("open", &[d]),
                };
                #[cfg(windows)]
                return match &file {
                    Some(f) => {
                        // git 给的相对路径用的是 /，资源管理器只认 \
                        let arg = std::ffi::OsString::from(format!("/select,{}", f.display().to_string().replace('/', "\\")));
                        spawn("explorer", &[arg.as_os_str()])
                    }
                    None => spawn("explorer", &[d]),
                };
                #[cfg(all(unix, not(target_os = "macos")))]
                return spawn("xdg-open", &[file.as_ref().and_then(|f| f.parent()).map(|p| p.as_os_str()).unwrap_or(d)]);
            }
            "terminal" => {
                #[cfg(target_os = "macos")]
                return spawn("open", &["-a".as_ref(), "Terminal".as_ref(), d]);
                #[cfg(windows)]
                return spawn("wt", &["-d".as_ref(), d])
                    // 没装 Windows Terminal 时，在仓库目录新开一个 cmd 窗口
                    .or_else(|_| {
                        use std::os::windows::process::CommandExt;
                        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
                        std::process::Command::new("cmd")
                            .arg("/K")
                            .current_dir(&dir)
                            .creation_flags(CREATE_NEW_CONSOLE)
                            .spawn()
                            .map(|_| ())
                            .map_err(|e| Error::from(tr!("无法打开终端：{e}", "Couldn't open a terminal: {e}")))
                    });
                #[cfg(all(unix, not(target_os = "macos")))]
                return spawn("x-terminal-emulator", &[]);
            }
            "editor" => {
                let editor = editor.as_deref().map(str::trim).filter(|e| !e.is_empty()).unwrap_or("code");
                let what = file.as_ref().map(|f| f.as_os_str()).unwrap_or(d);
                // Windows 上 code 之类是 .cmd 脚本，要通过 cmd 启动
                #[cfg(windows)]
                return spawn("cmd", &["/C".as_ref(), editor.as_ref(), what]);
                #[cfg(not(windows))]
                {
                    let r = spawn(editor, &[what]);
                    // 从访达启动的程序拿不到终端里的 PATH，VS Code 没装命令行工具时改用 open -a
                    #[cfg(target_os = "macos")]
                    if r.is_err() && editor == "code" {
                        return spawn("open", &["-a".as_ref(), "Visual Studio Code".as_ref(), what]);
                    }
                    r
                }
            }
            _ => Err(tr!("不支持的打开方式", "Unsupported target").into()),
        }
    })
    .await
}

/// 撤销最近一次操作
#[tauri::command]
async fn undo_last(state: State<'_, AppState>, repo: String) -> CmdResult<String> {
    let session = state.get(&repo).map_err(|e| e.to_string())?;
    blocking(move || {
        let (dir, git_dir, entry) = {
            let s = lock(&session)?;
            let e = s.undo.last().cloned().ok_or_else(|| tr!("没有可以撤销的操作", "Nothing to undo"))?;
            (s.workdir.clone(), s.git_dir.clone(), e)
        };
        // 之后又在别处改过仓库：这条和更早的记录都不再可靠，全部丢掉
        if !entry.is_current(&dir)? {
            lock(&session)?.undo.clear();
            return Err(tr!("无法撤销“{}”：在那之后仓库又有了变化（可能是在命令行或其他工具里操作的）", "Can't undo “{}”: the repository has changed since then (perhaps from the command line or another tool)", entry.label).into());
        }
        let msg = undo::undo(&dir, &git_dir, &entry)?;
        lock(&session)?.undo.pop();
        Ok(msg)
    })
    .await
}

#[tauri::command]
async fn conflict_file(state: State<'_, AppState>, repo: String, path: String) -> CmdResult<conflict::ConflictFile> {
    let (dir, _) = dirs_of(&state, &repo)?;
    blocking(move || conflict::load(&dir, &path)).await
}

#[tauri::command]
async fn file_history(state: State<'_, AppState>, repo: String, path: String) -> CmdResult<Vec<history::HistoryEntry>> {
    let (dir, _) = dirs_of(&state, &repo)?;
    blocking(move || history::file_history(&dir, &path, 5000)).await
}

#[tauri::command]
async fn blame(state: State<'_, AppState>, repo: String, path: String, rev: Option<String>) -> CmdResult<history::Blame> {
    let (dir, _) = dirs_of(&state, &repo)?;
    blocking(move || history::blame(&dir, &path, rev.as_deref())).await
}

/// 取消这个仓库正在执行的 git 操作
#[tauri::command]
fn cancel_op(state: State<'_, AppState>, repo: String) {
    if let Some(p) = state.running.lock().ok().and_then(|m| m.get(&repo).cloned()) {
        p.cancel();
    }
}

fn run_op(dir: &Path, git_dir: &Path, op: Op) -> Result<String, Error> {
    {
        let d = dir;
        let done = |r: Result<(), Error>| r.map(|_| String::new());
        match op {
            Op::Stage { paths } => done(ops::stage(d, &paths)),
            Op::StageAll => done(ops::stage_all(d)),
            Op::Unstage { paths } => done(ops::unstage(d, &paths)),
            Op::UnstageAll => done(ops::unstage_all(d)),
            Op::Discard { tracked, untracked } => done(ops::discard(d, &tracked, &untracked)),
            Op::StageHunk { path, header, staged } => {
                done(ops::apply_patch(d, &hunk(d, &path, &header, staged)?, true, staged))
            }
            Op::DiscardHunk { path, header } => done(ops::apply_patch(d, &hunk(d, &path, &header, false)?, false, true)),
            Op::Commit { message, amend } => done(ops::commit(d, &message, amend)),
            Op::Checkout { name } => done(ops::checkout(d, &name)),
            Op::CheckoutRemote { remote, local } => done(ops::checkout_remote(d, &remote, &local)),
            Op::CheckoutDetached { oid } => done(ops::checkout_detached(d, &oid)),
            Op::CreateBranch { name, start, checkout } => done(ops::create_branch(d, &name, &start, checkout)),
            Op::DeleteBranch { name, force } => done(ops::delete_branch(d, &name, force)),
            Op::DeleteRemoteBranch { name } => done(ops::delete_remote_branch(d, &name)),
            Op::RenameBranch { old, new, remote } => ops::rename_branch(d, &old, &new, remote),
            Op::Merge { target } => ops::merge(d, &target),
            Op::Rebase { onto } => ops::rebase(d, &onto),
            Op::Sequencer { kind, action } => ops::sequencer(d, &kind, &action),
            Op::CherryPick { oid } => ops::cherry_pick(d, &oid),
            Op::Revert { oid } => ops::revert(d, &oid),
            Op::Reset { oid, mode } => done(ops::reset(d, &oid, &mode)),
            Op::CreateTag { name, oid, message } => done(ops::create_tag(d, &name, &oid, message.as_deref())),
            Op::DeleteTag { name } => done(ops::delete_tag(d, &name)),
            Op::Fetch { quiet: true } => ops::without_prompt(|| ops::fetch(d)),
            Op::Fetch { quiet: false } => ops::fetch(d),
            Op::Pull => ops::pull(d),
            Op::Push { force } => ops::push(d, force),
            Op::StashPush { message, paths } => done(ops::stash_push(d, message.as_deref(), &paths)),
            Op::Stash { action, index } => done(ops::stash_action(d, &action, index)),
            Op::FlowInit { config } => done(flow::init(d, &config)),
            Op::FlowStart { kind, name, base } => flow::start(d, kind, &name, base.as_deref()),
            Op::FlowFinish { kind, name, options } => flow::finish(d, &git_dir, kind, &name, &options),
            Op::FlowPublish { kind, name } => flow::publish(d, kind, &name),
            Op::ResolveConflict { path, choices } => done(conflict::resolve(d, &path, &choices)),
            Op::ResolveSide { path, theirs } => done(conflict::resolve_side(d, &path, theirs)),
            Op::MergeInto { branch, source } => ops::merge_into(d, &branch, &source),
            Op::RebaseBranch { branch, onto } => ops::rebase_branch(d, &branch, &onto),
            Op::RebaseInteractive { from, steps } => rebase::run(d, git_dir, &from, &steps),
        }
    }
}

// ---------- 任务面板 ----------
// 全局一个 JSON 文件，存在应用自己的数据目录里，不属于任何仓库。
// 内容由界面定义，后端只负责原样读写。

fn tasks_file(app: &AppHandle) -> Result<PathBuf, Error> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("tasks.json"))
}

#[tauri::command]
async fn tasks_load(app: AppHandle) -> CmdResult<Option<serde_json::Value>> {
    blocking(move || {
        let path = tasks_file(&app)?;
        match std::fs::read(&path) {
            Ok(data) => Ok(Some(serde_json::from_slice(&data)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
    .await
}

#[tauri::command]
async fn tasks_save(app: AppHandle, board: serde_json::Value) -> CmdResult<()> {
    blocking(move || {
        let path = tasks_file(&app)?;
        // 先写临时文件再改名，保存到一半崩溃也不会把原文件写坏
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&board)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    })
    .await
}

/// 启动时要打开的仓库：命令行参数（gittree /path/to/repo）或环境变量 GITTREE_REPO
#[tauri::command]
fn initial_repo() -> Option<String> {
    std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .or_else(|| std::env::var("GITTREE_REPO").ok())
        .filter(|p| Path::new(p).is_dir())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 被 git 当作询问密码的程序启动时，问完直接退出，不打开窗口
    if let Some(code) = askpass::helper_main() {
        std::process::exit(code);
    }
    fix_path();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|w, e| {
            // 开着后台运行时，关窗口只是隐藏，定时提醒照常发
            if let tauri::WindowEvent::CloseRequested { api, .. } = e
                && background::keep_running()
            {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .setup(|app| {
            ops::set_extra_env(askpass::start(app.handle().clone()));
            Ok(())
        })
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            set_lang,
            open_repo,
            close_repo,
            refresh_repo,
            set_focus,
            cancel_op,
            undo_last,
            askpass::askpass_reply,
            background::set_keep_running,
            clone_repo,
            init_repo,
            rebase_plan,
            open_in,
            conflict_file,
            file_history,
            blame,
            get_rows,
            search,
            row_of,
            commit_details,
            commit_file_diff,
            status,
            workdir_diff,
            stash_list,
            last_commit_message,
            git_op,
            flow_config,
            tasks_load,
            tasks_save,
            initial_repo,
        ])
        .build(tauri::generate_context!())
        .expect("启动失败")
        .run(|_app, _e| {
            // macOS 上窗口隐藏后点程序坞图标，把窗口显示回来
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _e {
                background::show_main(_app);
            }
        });
}

/// 从访达 / 程序坞启动时，macOS 只给最基本的 PATH，找不到 Homebrew 装的 git 和编辑器命令。
/// 启动时把常见的目录补上（其他线程启动之前修改环境变量是安全的）。
fn fix_path() {
    #[cfg(target_os = "macos")]
    {
        let cur = std::env::var("PATH").unwrap_or_default();
        let mut parts: Vec<&str> = cur.split(':').filter(|p| !p.is_empty()).collect();
        for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/Applications/Visual Studio Code.app/Contents/Resources/app/bin"] {
            if !parts.contains(&extra) {
                parts.push(extra);
            }
        }
        // SAFETY: 在创建任何其他线程之前调用
        unsafe { std::env::set_var("PATH", parts.join(":")) };
    }
}
