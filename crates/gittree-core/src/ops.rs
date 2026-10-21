//! 会修改仓库的操作都交给系统安装的 git 命令执行：
//! 这样 hooks、GPG 签名、SSH 和凭据管理器都和命令行里的行为完全一致。

use crate::tr;
use crate::log::Error;
use serde::Serialize;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// 每个 git 命令都带上的额外环境变量（界面程序用它设置 GIT_ASKPASS，需要密码时弹窗询问）
static EXTRA_ENV: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();

thread_local! {
    static NO_PROMPT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// f 执行期间当前线程里的 git 命令不弹窗询问密码，需要时直接失败
pub fn without_prompt<T>(f: impl FnOnce() -> T) -> T {
    NO_PROMPT.with(|n| n.set(true));
    let out = f();
    NO_PROMPT.with(|n| n.set(false));
    out
}

/// 程序启动时调用一次
pub fn set_extra_env(envs: Vec<(String, String)>) {
    let _ = EXTRA_ENV.set(envs);
}

fn command(dir: &Path) -> Command {
    let mut c = Command::new("git");
    c.current_dir(dir)
        // 没有终端可以输入：需要密码时只通过 GIT_ASKPASS 询问，没有设置就直接失败
        .env("GIT_TERMINAL_PROMPT", "0")
        // 不打开编辑器（合并提交说明等）
        .env("GIT_EDITOR", "true")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // 后台操作（自动获取）不弹窗问密码
    if !NO_PROMPT.with(|n| n.get()) {
        for (k, v) in EXTRA_ENV.get().into_iter().flatten() {
            c.env(k, v);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // 不弹出黑色控制台窗口
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// 执行 git 命令，失败时把 git 的错误输出作为错误信息返回
pub fn git(dir: &Path, args: &[&str]) -> Result<String, Error> {
    git_with_input(dir, args, None)
}

pub fn git_with_input(dir: &Path, args: &[&str], input: Option<&[u8]>) -> Result<String, Error> {
    git_full(dir, args, input, &[])
}

/// 带额外环境变量执行 git（例如交互式变基用 GIT_SEQUENCE_EDITOR 写入待办列表）
pub fn git_env(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Result<String, Error> {
    git_full(dir, args, None, envs)
}

fn git_full(dir: &Path, args: &[&str], input: Option<&[u8]>, envs: &[(&str, &str)]) -> Result<String, Error> {
    git_raw(dir, args, input, envs).map(|out| String::from_utf8_lossy(&out).into_owned())
}

/// 输出是二进制内容时用（例如 git lfs smudge 输出的图片）
pub fn git_bytes(dir: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, Error> {
    git_raw(dir, args, input, &[])
}

fn git_raw(dir: &Path, args: &[&str], input: Option<&[u8]>, envs: &[(&str, &str)]) -> Result<Vec<u8>, Error> {
    let mut c = command(dir);
    c.args(args);
    for (k, v) in envs {
        c.env(k, v);
    }
    if input.is_some() {
        c.stdin(Stdio::piped());
    }
    let mut child = c.spawn().map_err(|e| tr!("无法运行 git，请确认已安装 Git 并加入 PATH：{e}", "Couldn't run git. Make sure Git is installed and on PATH: {e}"))?;
    if let Some(data) = input {
        child.stdin.take().unwrap().write_all(data)?;
    }
    let progress = PROGRESS.with(|p| p.borrow().clone());
    let (status, stdout, stderr) = match progress {
        None => {
            let out = child.wait_with_output()?;
            (out.status, out.stdout, out.stderr)
        }
        Some(p) => run_streaming(child, args, &p)?,
    };
    if status.success() {
        Ok(stdout)
    } else if progress_cancelled() {
        Err(tr!("已取消", "Cancelled").into())
    } else {
        let stdout = String::from_utf8_lossy(&stdout);
        let stderr = String::from_utf8_lossy(&stderr);
        let msg = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
        Err(tr!("git {} 失败：\n{}", "git {} failed:\n{}", args.first().unwrap_or(&""), msg).into())
    }
}

// ---------- 进度 ----------
// 获取、拉取、推送这类慢操作把 git 的 stderr 逐行转发给界面，
// 这样能看到“正在接收对象 45%”之类的进度，也能中途取消。

/// 进度回调：每一行（或 \r 刷新的一段）输出调用一次
pub type ProgressSink = dyn Fn(ProgressLine) + Send + Sync;

#[derive(Debug, Clone, Serialize)]
pub struct ProgressLine {
    pub text: String,
    /// true 表示这一行会被下一行原地覆盖（git 用 \r 刷新百分比）
    pub transient: bool,
}

/// 一次操作的进度上下文，可在别的线程里调用 cancel
pub struct Progress {
    sink: Box<ProgressSink>,
    cancelled: AtomicBool,
    pid: Mutex<Option<u32>>,
}

impl Progress {
    pub fn new(sink: impl Fn(ProgressLine) + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self { sink: Box::new(sink), cancelled: AtomicBool::new(false), pid: Mutex::new(None) })
    }

    pub fn emit(&self, text: impl Into<String>) {
        (self.sink)(ProgressLine { text: text.into(), transient: false });
    }

    /// 结束正在运行的 git 进程，后续的 git 命令也不再执行
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(pid) = *self.pid.lock().unwrap() {
            kill(pid);
        }
    }
}

thread_local! {
    static PROGRESS: RefCell<Option<Arc<Progress>>> = const { RefCell::new(None) };
}

/// 在 f 执行期间，当前线程里运行的 git 命令都把输出转发给 progress
pub fn with_progress<T>(progress: Arc<Progress>, f: impl FnOnce() -> T) -> T {
    PROGRESS.with(|p| *p.borrow_mut() = Some(progress));
    let out = f();
    PROGRESS.with(|p| *p.borrow_mut() = None);
    out
}

fn progress_cancelled() -> bool {
    PROGRESS.with(|p| p.borrow().as_ref().is_some_and(|p| p.cancelled.load(Ordering::SeqCst)))
}

fn kill(pid: u32) {
    #[cfg(unix)]
    let _ = Command::new("kill").arg(pid.to_string()).status();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // /T 连同 git 启动的子进程（ssh、git-remote-https）一起结束
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .status();
    }
}

fn run_streaming(
    mut child: std::process::Child,
    args: &[&str],
    p: &Progress,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), Error> {
    if p.cancelled.load(Ordering::SeqCst) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(tr!("已取消", "Cancelled").into());
    }
    *p.pid.lock().unwrap() = Some(child.id());
    p.emit(format!("$ git {}", args.join(" ")));
    // stdout 在另一个线程里读完，避免两个管道互相堵住
    let mut out = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        buf
    });
    let mut err = child.stderr.take().unwrap();
    let mut all = Vec::new();
    let mut line = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = match err.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        all.extend_from_slice(&buf[..n]);
        for &b in &buf[..n] {
            if b == b'\r' || b == b'\n' {
                if !line.is_empty() {
                    let text = String::from_utf8_lossy(&line).trim_end().to_string();
                    (p.sink)(ProgressLine { text, transient: b == b'\r' });
                    line.clear();
                }
            } else {
                line.push(b);
            }
        }
    }
    if !line.is_empty() {
        p.emit(String::from_utf8_lossy(&line).trim_end().to_string());
    }
    let status = child.wait()?;
    *p.pid.lock().unwrap() = None;
    let stdout = reader.join().unwrap_or_default();
    Ok((status, stdout, all))
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusEntry {
    pub path: String,
    pub old_path: Option<String>,
    /// 暂存区状态：'.' 未改动，M A D R C T，'?' 未跟踪
    pub index: char,
    /// 工作区状态
    pub worktree: char,
    pub conflicted: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct Status {
    pub staged: Vec<StatusEntry>,
    pub unstaged: Vec<StatusEntry>,
    pub conflicted: Vec<StatusEntry>,
    /// 正在进行的操作：merge、rebase、cherry-pick、revert
    pub in_progress: Option<String>,
}

pub fn status(dir: &Path, git_dir: &Path) -> Result<Status, Error> {
    let out = git(dir, &["status", "--porcelain=v2", "-z", "--untracked-files=all", "--no-renames"])?;
    let mut st = parse_status(&out);
    st.in_progress = if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        Some("rebase".into())
    } else if git_dir.join("MERGE_HEAD").exists() {
        Some("merge".into())
    } else if git_dir.join("CHERRY_PICK_HEAD").exists() {
        Some("cherry-pick".into())
    } else if git_dir.join("REVERT_HEAD").exists() {
        Some("revert".into())
    } else {
        None
    };
    Ok(st)
}

fn parse_status(out: &str) -> Status {
    let mut st = Status::default();
    let mut fields = out.split('\0');
    while let Some(rec) = fields.next() {
        if rec.is_empty() {
            continue;
        }
        let (kind, rest) = rec.split_at(1);
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        let entry = match kind {
            // 1 XY sub mH mI mW hH hI path
            "1" => {
                let parts: Vec<&str> = rest.splitn(8, ' ').collect();
                if parts.len() < 8 {
                    continue;
                }
                xy_entry(parts[0], parts[7], None, false)
            }
            // 2 XY sub mH mI mW hH hI Xscore path \0 origPath
            "2" => {
                let parts: Vec<&str> = rest.splitn(9, ' ').collect();
                let orig = fields.next().map(str::to_string);
                if parts.len() < 9 {
                    continue;
                }
                xy_entry(parts[0], parts[8], orig, false)
            }
            // u XY sub m1 m2 m3 mW h1 h2 h3 path
            "u" => {
                let parts: Vec<&str> = rest.splitn(10, ' ').collect();
                if parts.len() < 10 {
                    continue;
                }
                xy_entry(parts[0], parts[9], None, true)
            }
            "?" => StatusEntry {
                path: rest.to_string(),
                old_path: None,
                index: '?',
                worktree: '?',
                conflicted: false,
            },
            _ => continue,
        };
        if entry.conflicted {
            st.conflicted.push(entry);
            continue;
        }
        if entry.index != '.' && entry.index != '?' {
            st.staged.push(entry.clone());
        }
        if entry.worktree != '.' {
            st.unstaged.push(entry);
        }
    }
    st
}

fn xy_entry(xy: &str, path: &str, old: Option<String>, conflicted: bool) -> StatusEntry {
    let mut c = xy.chars();
    StatusEntry {
        path: path.to_string(),
        old_path: old,
        index: c.next().unwrap_or('.'),
        worktree: c.next().unwrap_or('.'),
        conflicted,
    }
}

fn with_paths<'a>(mut args: Vec<&'a str>, paths: &'a [String]) -> Vec<&'a str> {
    args.push("--");
    args.extend(paths.iter().map(String::as_str));
    args
}

pub fn stage(dir: &Path, paths: &[String]) -> Result<(), Error> {
    git(dir, &with_paths(vec!["add", "-A"], paths))?;
    Ok(())
}

pub fn stage_all(dir: &Path) -> Result<(), Error> {
    git(dir, &["add", "-A"])?;
    Ok(())
}

pub fn unstage(dir: &Path, paths: &[String]) -> Result<(), Error> {
    if git(dir, &["rev-parse", "--verify", "-q", "HEAD"]).is_ok() {
        git(dir, &with_paths(vec!["restore", "--staged"], paths))?;
    } else {
        // 还没有任何提交时没有 HEAD 可以恢复
        git(dir, &with_paths(vec!["rm", "--cached", "-r", "-q"], paths))?;
    }
    Ok(())
}

pub fn unstage_all(dir: &Path) -> Result<(), Error> {
    if git(dir, &["rev-parse", "--verify", "-q", "HEAD"]).is_ok() {
        git(dir, &["reset", "-q"])?;
    } else {
        git(dir, &["rm", "--cached", "-r", "-q", "."])?;
    }
    Ok(())
}

/// 丢弃工作区改动。未跟踪的文件会被删除。
pub fn discard(dir: &Path, tracked: &[String], untracked: &[String]) -> Result<(), Error> {
    if !tracked.is_empty() {
        git(dir, &with_paths(vec!["restore", "--worktree"], tracked))?;
    }
    if !untracked.is_empty() {
        git(dir, &with_paths(vec!["clean", "-f", "-q"], untracked))?;
    }
    Ok(())
}

/// 对暂存区（cached = true）或工作区应用补丁；reverse = true 表示撤销
pub fn apply_patch(dir: &Path, patch: &[u8], cached: bool, reverse: bool) -> Result<(), Error> {
    let mut args = vec!["apply", "--whitespace=nowarn"];
    if cached {
        args.push("--cached");
    }
    if reverse {
        args.push("--reverse");
    }
    args.push("-");
    git_with_input(dir, &args, Some(patch))?;
    Ok(())
}

pub fn commit(dir: &Path, message: &str, amend: bool) -> Result<(), Error> {
    let mut args = vec!["commit", "-q", "-F", "-"];
    if amend {
        args.push("--amend");
    }
    git_with_input(dir, &args, Some(message.as_bytes()))?;
    Ok(())
}

pub fn checkout(dir: &Path, branch: &str) -> Result<(), Error> {
    git(dir, &["switch", branch])?;
    Ok(())
}

/// 检出远程分支：创建同名本地分支并跟踪它
pub fn checkout_remote(dir: &Path, remote_branch: &str, local: &str) -> Result<(), Error> {
    git(dir, &["switch", "-c", local, "--track", remote_branch])?;
    Ok(())
}

pub fn checkout_detached(dir: &Path, oid: &str) -> Result<(), Error> {
    git(dir, &["switch", "--detach", oid])?;
    Ok(())
}

/// 先把本地改动（包括未跟踪的文件）stash 起来再执行 f，用于本地改动会被切换分支覆盖的情况。
/// pop 为 true 时完成后把改动恢复回来；f 失败时也会恢复，相当于什么都没做。
pub fn with_stash(dir: &Path, what: &str, pop: bool, f: impl FnOnce() -> Result<String, Error>) -> Result<String, Error> {
    let stash_oid = || git(dir, &["rev-parse", "-q", "--verify", "refs/stash"]).ok();
    let before = stash_oid();
    let message = tr!("GitTree：{what} 前自动保存", "GitTree: saved before {what}");
    git(dir, &["stash", "push", "--include-untracked", "-m", &message])?;
    // 没有改动时 git 不会新建 stash
    let stashed = stash_oid().is_some_and(|o| Some(&o) != before.as_ref());
    let out = match f() {
        Ok(out) => out,
        Err(e) => {
            if stashed {
                let _ = git(dir, &["stash", "pop"]);
            }
            return Err(e);
        }
    };
    if !stashed {
        return Ok(out);
    }
    if !pop {
        return Ok(tr!(
            "原来的改动保存在 stash 里（{message}），需要时在左侧栏的 Stash 里恢复",
            "Your changes are saved in the stash ({message}). Restore them from Stash in the sidebar when needed"
        ));
    }
    // 恢复时冲突：git 会保留这条 stash，冲突留在工作区里给用户解决
    git(dir, &["stash", "pop"]).map_err(|e| {
        tr!(
            "{what}已经完成，但恢复改动时有冲突。请解决冲突；原来的改动还保存在 stash 里，确认没问题后可以把它删掉。\n\n{e}",
            "{what} is done, but restoring your changes caused conflicts. Resolve them; your original changes are still in the stash and can be dropped once you're done.\n\n{e}"
        )
    })?;
    Ok(out)
}

pub fn create_branch(dir: &Path, name: &str, start: &str, checkout: bool) -> Result<(), Error> {
    if checkout {
        git(dir, &["switch", "-c", name, start])?;
    } else {
        git(dir, &["branch", name, start])?;
    }
    Ok(())
}

pub fn delete_branch(dir: &Path, name: &str, force: bool) -> Result<(), Error> {
    git(dir, &["branch", if force { "-D" } else { "-d" }, name])?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct Remote {
    pub name: String,
    pub url: String,
    /// 和 url 不同时才有（git remote set-url --push）
    pub push_url: Option<String>,
}

/// 所有远程仓库及地址
pub fn remotes(dir: &Path) -> Result<Vec<Remote>, Error> {
    let out = git(dir, &["remote", "-v"])?;
    let mut list: Vec<Remote> = Vec::new();
    for line in out.lines() {
        // origin\tgit@host:a/b.git (fetch)
        let Some((name, rest)) = line.split_once('\t') else { continue };
        let Some((url, kind)) = rest.rsplit_once(' ') else { continue };
        let i = match list.iter().position(|r| r.name == name) {
            Some(i) => i,
            None => {
                list.push(Remote { name: name.to_string(), url: String::new(), push_url: None });
                list.len() - 1
            }
        };
        if kind == "(fetch)" {
            list[i].url = url.to_string();
        } else {
            list[i].push_url = Some(url.to_string());
        }
    }
    for r in &mut list {
        if r.push_url.as_deref() == Some(r.url.as_str()) {
            r.push_url = None;
        }
    }
    Ok(list)
}

pub fn add_remote(dir: &Path, name: &str, url: &str) -> Result<(), Error> {
    git(dir, &["remote", "add", name.trim(), url.trim()])?;
    Ok(())
}

pub fn remove_remote(dir: &Path, name: &str) -> Result<(), Error> {
    git(dir, &["remote", "remove", name])?;
    Ok(())
}

pub fn rename_remote(dir: &Path, old: &str, new: &str) -> Result<(), Error> {
    git(dir, &["remote", "rename", old, new.trim()])?;
    Ok(())
}

pub fn set_remote_url(dir: &Path, name: &str, url: &str) -> Result<(), Error> {
    git(dir, &["remote", "set-url", name, url.trim()])?;
    Ok(())
}

/// 推送一个标签；tag 为空时推送所有标签
pub fn push_tag(dir: &Path, remote: &str, tag: Option<&str>) -> Result<String, Error> {
    match tag {
        Some(tag) => git(dir, &["push", "--progress", remote, &format!("refs/tags/{tag}")]),
        None => git(dir, &["push", "--progress", remote, "--tags"]),
    }
}

pub fn delete_remote_tag(dir: &Path, remote: &str, tag: &str) -> Result<String, Error> {
    git(dir, &["push", "--progress", remote, "--delete", &format!("refs/tags/{tag}")])
}

/// 分支跟踪的远程分支：(远程名, 远程上的分支名)
fn tracking(dir: &Path, branch: &str) -> Option<(String, String)> {
    let remote = git(dir, &["config", "--get", &format!("branch.{branch}.remote")]).ok()?;
    let merge = git(dir, &["config", "--get", &format!("branch.{branch}.merge")]).ok()?;
    let (remote, merge) = (remote.trim(), merge.trim());
    // 跟踪的是本地另一个分支（remote 为 .）时不算
    if remote.is_empty() || remote == "." {
        return None;
    }
    Some((remote.to_string(), merge.strip_prefix("refs/heads/").unwrap_or(merge).to_string()))
}

/// 重命名本地分支。remote 为 true 且分支有上游时，远程分支也改名：
/// 推送新名字并设为上游，再删掉远程的旧名字
pub fn rename_branch(dir: &Path, old: &str, new: &str, remote: bool) -> Result<String, Error> {
    let upstream = if remote { tracking(dir, old) } else { None };
    git(dir, &["branch", "-m", old, new])?;
    let Some((r, old_remote)) = upstream else {
        return Ok(String::new());
    };
    git(dir, &["push", "--progress", "-u", &r, &format!("{new}:refs/heads/{new}")])?;
    git(dir, &["push", "--progress", &r, "--delete", &old_remote])?;
    Ok(tr!(
        "已把远程分支 {r}/{old_remote} 改名为 {r}/{new}",
        "Renamed the remote branch {r}/{old_remote} to {r}/{new}"
    ))
}

/// 删除远程分支，例如 origin/feature
pub fn delete_remote_branch(dir: &Path, remote_branch: &str) -> Result<(), Error> {
    let (remote, branch) = remote_branch.split_once('/').ok_or_else(|| tr!("远程分支名称格式不对", "Invalid remote branch name"))?;
    git(dir, &["push", remote, "--delete", branch])?;
    Ok(())
}

pub fn merge(dir: &Path, target: &str) -> Result<String, Error> {
    git(dir, &["merge", "--no-edit", target])
}

pub fn rebase(dir: &Path, onto: &str) -> Result<String, Error> {
    git(dir, &["rebase", onto])
}

/// 中止或继续正在进行的 merge / rebase / cherry-pick / revert
pub fn sequencer(dir: &Path, op: &str, action: &str) -> Result<String, Error> {
    if !matches!(op, "merge" | "rebase" | "cherry-pick" | "revert") || !matches!(action, "abort" | "continue") {
        return Err(tr!("不支持的操作", "Unsupported operation").into());
    }
    let flag = format!("--{action}");
    git(dir, &[op, &flag])
}

pub fn cherry_pick(dir: &Path, oid: &str) -> Result<String, Error> {
    git(dir, &["cherry-pick", oid])
}

pub fn revert(dir: &Path, oid: &str) -> Result<String, Error> {
    git(dir, &["revert", "--no-edit", oid])
}

pub fn reset(dir: &Path, oid: &str, mode: &str) -> Result<(), Error> {
    if !matches!(mode, "soft" | "mixed" | "hard") {
        return Err(tr!("不支持的重置模式", "Unsupported reset mode").into());
    }
    git(dir, &["reset", &format!("--{mode}"), oid])?;
    Ok(())
}

pub fn create_tag(dir: &Path, name: &str, oid: &str, message: Option<&str>) -> Result<(), Error> {
    match message.filter(|m| !m.trim().is_empty()) {
        Some(m) => git_with_input(dir, &["tag", "-a", name, oid, "-F", "-"], Some(m.as_bytes()))?,
        None => git(dir, &["tag", name, oid])?,
    };
    Ok(())
}

pub fn delete_tag(dir: &Path, name: &str) -> Result<(), Error> {
    git(dir, &["tag", "-d", name])?;
    Ok(())
}

pub fn fetch(dir: &Path) -> Result<String, Error> {
    git(dir, &["fetch", "--all", "--prune", "--tags", "--progress"])
}

/// 克隆到 parent/name，返回仓库路径
pub fn clone(parent: &Path, url: &str, name: &str) -> Result<std::path::PathBuf, Error> {
    let target = parent.join(name);
    if target.exists() && std::fs::read_dir(&target).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(tr!("{} 已经存在并且不是空文件夹", "{} already exists and isn't an empty folder", target.display()).into());
    }
    std::fs::create_dir_all(parent)?;
    git(parent, &["clone", "--progress", url, name])?;
    Ok(target)
}

/// 新建一个空仓库
pub fn init(path: &Path) -> Result<(), Error> {
    std::fs::create_dir_all(path)?;
    git(path, &["init"])?;
    Ok(())
}

/// 先切换到 branch，再把 source 合并进来
pub fn merge_into(dir: &Path, branch: &str, source: &str) -> Result<String, Error> {
    checkout(dir, branch)?;
    merge(dir, source)
}

/// 先切换到 branch，再把它变基到 onto
pub fn rebase_branch(dir: &Path, branch: &str, onto: &str) -> Result<String, Error> {
    checkout(dir, branch)?;
    rebase(dir, onto)
}

pub fn pull(dir: &Path) -> Result<String, Error> {
    git(dir, &["pull", "--no-edit", "--progress"])
}

/// 推送当前分支；没有上游分支时推送到 origin 并设置跟踪
pub fn push(dir: &Path, force: bool) -> Result<String, Error> {
    let has_upstream = git(dir, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).is_ok();
    let mut args = vec!["push", "--progress"];
    if force {
        args.push("--force-with-lease");
    }
    if !has_upstream {
        let remotes = git(dir, &["remote"])?;
        let remote = remotes.lines().find(|r| *r == "origin").or_else(|| remotes.lines().next());
        let Some(remote) = remote else {
            return Err(tr!("这个仓库还没有配置远程仓库", "This repository has no remotes").into());
        };
        args.extend(["-u", remote, "HEAD"]);
        return git(dir, &args);
    }
    git(dir, &args)
}

/// 在仓库根目录的 .gitignore 末尾加规则（已经有了的不重复加）
pub fn ignore(dir: &Path, patterns: &[String]) -> Result<(), Error> {
    let patterns: Vec<&str> = patterns.iter().map(|p| p.trim()).collect();
    if patterns.is_empty() || patterns.iter().any(|p| p.is_empty() || p.contains('\n')) {
        return Err(tr!("忽略规则不能为空", "The ignore rule can't be empty").into());
    }
    let path = dir.join(".gitignore");
    let mut text = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    let existing: Vec<String> = String::from_utf8_lossy(&text).lines().map(|l| l.trim().to_string()).collect();
    let new: Vec<&str> = patterns.into_iter().filter(|p| !existing.iter().any(|e| e == p)).collect();
    if new.is_empty() {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with(b"\n") {
        text.push(b'\n');
    }
    for p in new {
        text.extend_from_slice(p.as_bytes());
        text.push(b'\n');
    }
    std::fs::write(&path, text)?;
    Ok(())
}

/// 把路径转成 .gitignore 里只匹配它本身的规则：以 / 开头，转义通配符
pub fn ignore_pattern_for(path: &str) -> String {
    let mut out = String::from("/");
    for c in path.chars() {
        if matches!(c, '*' | '?' | '[' | '\\' | '!' | '#') {
            out.push('\\');
        }
        out.push(c);
    }
    // 结尾的空格会被 git 忽略，要转义
    if out.ends_with(' ') {
        out.pop();
        out.push_str("\\ ");
    }
    out
}

#[derive(Debug, Serialize)]
pub struct Signature {
    /// G 有效，U 有效但不确定是谁的密钥，B 无效，X/Y 过期，R 密钥已吊销，E 无法验证（缺少公钥），N 没有签名
    pub status: char,
    pub signer: String,
    pub key: String,
    /// gpg 或 ssh
    pub format: String,
}

/// 提交的签名验证结果。没有签名时返回 None（不用启动 gpg）
pub fn signature(dir: &Path, oid: &str) -> Result<Option<Signature>, Error> {
    let repo = git2::Repository::open(dir)?;
    let Ok((sig, _)) = repo.extract_signature(&git2::Oid::from_str(oid)?, None) else {
        return Ok(None);
    };
    let format = if sig.starts_with(b"-----BEGIN SSH SIGNATURE") { "ssh" } else { "gpg" };
    let out = git(dir, &["log", "-1", "--format=%G?%x00%GS%x00%GK", oid])?;
    let mut p = out.trim_end().split('\0');
    Ok(Some(Signature {
        status: p.next().and_then(|s| s.chars().next()).unwrap_or('E'),
        signer: p.next().unwrap_or("").to_string(),
        key: p.next().unwrap_or("").to_string(),
        format: format.into(),
    }))
}

/// 从 PR 的远程引用新建本地分支并切换过去（例如 refs/pull/12/head → pr/12）
pub fn checkout_pull(dir: &Path, remote: &str, remote_ref: &str, local: &str) -> Result<String, Error> {
    let spec = format!("+{remote_ref}:refs/heads/{local}");
    // 已经在这个分支上时 fetch 不能直接改它，先切走不划算：改用 pull 的方式更新
    let current = git(dir, &["symbolic-ref", "-q", "--short", "HEAD"]).unwrap_or_default();
    if current.trim() == local {
        return git(dir, &["pull", "--no-edit", "--progress", remote, remote_ref]);
    }
    git(dir, &["fetch", "--progress", remote, &spec])?;
    git(dir, &["switch", local])
}

#[derive(Debug, Serialize)]
pub struct Stash {
    pub index: u32,
    pub name: String,
    pub message: String,
    pub oid: String,
}

pub fn stash_list(dir: &Path) -> Result<Vec<Stash>, Error> {
    let out = git(dir, &["stash", "list", "--format=%gd%x00%s%x00%H"])?;
    Ok(out
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let mut p = l.split('\0');
            Some(Stash {
                index: i as u32,
                name: p.next()?.to_string(),
                message: p.next()?.to_string(),
                oid: p.next()?.to_string(),
            })
        })
        .collect())
}

/// stash 改动。paths 为空时 stash 全部（包括未跟踪的文件），否则只 stash 这些文件
pub fn stash_push(dir: &Path, message: Option<&str>, paths: &[String]) -> Result<(), Error> {
    let mut args = vec!["stash", "push", "--include-untracked"];
    if let Some(m) = message.filter(|m| !m.trim().is_empty()) {
        args.extend(["-m", m]);
    }
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    git(dir, &args)?;
    Ok(())
}

pub fn stash_action(dir: &Path, action: &str, index: u32) -> Result<(), Error> {
    if !matches!(action, "pop" | "apply" | "drop") {
        return Err(tr!("不支持的 stash 操作", "Unsupported stash operation").into());
    }
    git(dir, &["stash", action, &format!("stash@{{{index}}}")])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v2() {
        let out = "1 M. N... 100644 100644 100644 aaa bbb src/main.rs\0\
1 .M N... 100644 100644 100644 aaa bbb 文档/说明 文件.md\0\
1 MM N... 100644 100644 100644 aaa bbb both.txt\0\
u UU N... 100644 100644 100644 100644 a b c conflict.txt\0\
? new file.txt\0";
        let st = parse_status(out);
        let staged: Vec<_> = st.staged.iter().map(|e| e.path.as_str()).collect();
        let unstaged: Vec<_> = st.unstaged.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(staged, ["src/main.rs", "both.txt"]);
        assert_eq!(unstaged, ["文档/说明 文件.md", "both.txt", "new file.txt"]);
        assert_eq!(st.conflicted[0].path, "conflict.txt");
        assert_eq!(st.unstaged[2].index, '?');
    }
}
