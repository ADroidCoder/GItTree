use gittree_core::{conflict, history, ops, undo};
use std::path::{Path, PathBuf};

fn temp_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gittree-undo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    ops::git(&dir, &["init", "-q", "-b", "main"]).unwrap();
    ops::git(&dir, &["config", "user.name", "测试者"]).unwrap();
    ops::git(&dir, &["config", "user.email", "t@example.com"]).unwrap();
    ops::git(&dir, &["config", "commit.gpgsign", "false"]).unwrap();
    // Git for Windows 默认会把换行符转换成 \r\n，测试里按原样比较文件内容
    ops::git(&dir, &["config", "core.autocrlf", "false"]).unwrap();
    dir
}

fn commit_file(dir: &Path, file: &str, content: &str, msg: &str) {
    std::fs::write(dir.join(file), content).unwrap();
    ops::stage_all(dir).unwrap();
    ops::commit(dir, msg, false).unwrap();
}

fn rev(dir: &Path, r: &str) -> String {
    ops::git(dir, &["rev-parse", r]).unwrap().trim().to_string()
}

/// 模拟界面上执行一个可撤销的操作
fn step(dir: &Path, stack: &mut Vec<undo::Entry>, label: &str, mode: undo::Mode, f: impl FnOnce()) {
    let before = undo::snapshot(dir).unwrap();
    f();
    if let Some(e) = undo::record(label, mode, before, undo::snapshot(dir).unwrap()) {
        stack.push(e);
    }
}

#[test]
fn undo_commit_branch_merge_and_delete() {
    let dir = temp_repo("ops");
    let git_dir = dir.join(".git");
    commit_file(&dir, "a.txt", "1\n", "初始");
    let base = rev(&dir, "HEAD");
    let mut stack = Vec::new();

    // 提交 → 撤销后改动回到暂存区
    std::fs::write(dir.join("a.txt"), "2\n").unwrap();
    ops::stage_all(&dir).unwrap();
    step(&dir, &mut stack, "提交", undo::Mode::Soft, || ops::commit(&dir, "第二个", false).unwrap());
    let e = stack.pop().unwrap();
    assert!(e.is_current(&dir).unwrap());
    undo::undo(&dir, &git_dir, &e).unwrap();
    assert_eq!(rev(&dir, "HEAD"), base);
    assert_eq!(ops::git(&dir, &["diff", "--cached", "--name-only"]).unwrap().trim(), "a.txt");
    ops::commit(&dir, "第二个", false).unwrap();
    let second = rev(&dir, "HEAD");

    // 新建并切换分支 → 撤销后回到 main，分支被删除
    step(&dir, &mut stack, "新建分支", undo::Mode::Keep, || ops::create_branch(&dir, "dev", "HEAD", true).unwrap());
    commit_file(&dir, "b.txt", "b\n", "dev 上的提交");
    // 在 dev 上又提交了，这条记录已经过期
    let e = stack.pop().unwrap();
    assert!(!e.is_current(&dir).unwrap());
    assert!(undo::undo(&dir, &git_dir, &e).is_err());

    // 合并 → 撤销后 main 回到合并前
    ops::checkout(&dir, "main").unwrap();
    commit_file(&dir, "c.txt", "c\n", "main 上的提交");
    let main_before = rev(&dir, "main");
    step(&dir, &mut stack, "合并", undo::Mode::Keep, || {
        ops::merge(&dir, "dev").unwrap();
    });
    assert_ne!(rev(&dir, "main"), main_before);
    undo::undo(&dir, &git_dir, &stack.pop().unwrap()).unwrap();
    assert_eq!(rev(&dir, "main"), main_before);
    assert!(!dir.join("b.txt").exists());

    // 删除分支 → 撤销后恢复到原来的提交
    let dev = rev(&dir, "dev");
    step(&dir, &mut stack, "删除分支", undo::Mode::Keep, || ops::delete_branch(&dir, "dev", true).unwrap());
    assert!(ops::git(&dir, &["rev-parse", "--verify", "-q", "refs/heads/dev"]).is_err());
    undo::undo(&dir, &git_dir, &stack.pop().unwrap()).unwrap();
    assert_eq!(rev(&dir, "dev"), dev);

    // 切换分支 → 撤销后切回来
    step(&dir, &mut stack, "切换", undo::Mode::Keep, || ops::checkout(&dir, "dev").unwrap());
    undo::undo(&dir, &git_dir, &stack.pop().unwrap()).unwrap();
    assert_eq!(ops::git(&dir, &["branch", "--show-current"]).unwrap().trim(), "main");
    let _ = second;
}

#[test]
fn undo_flow_finish_restores_feature_branch() {
    use gittree_core::flow;
    let dir = temp_repo("flow");
    let git_dir = dir.join(".git");
    commit_file(&dir, "a.txt", "1\n", "初始");
    flow::init(&dir, &flow::config(&dir)).unwrap();
    flow::start(&dir, flow::Kind::Feature, "login", None).unwrap();
    commit_file(&dir, "login.txt", "x\n", "登录");
    let feature = rev(&dir, "feature/login");
    let develop = rev(&dir, "develop");

    let mut stack = Vec::new();
    step(&dir, &mut stack, "完成", undo::Mode::Keep, || {
        flow::finish(&dir, &git_dir, flow::Kind::Feature, "login", &Default::default()).unwrap();
    });
    assert!(ops::git(&dir, &["rev-parse", "--verify", "-q", "refs/heads/feature/login"]).is_err());
    undo::undo(&dir, &git_dir, &stack.pop().unwrap()).unwrap();
    assert_eq!(rev(&dir, "feature/login"), feature);
    assert_eq!(rev(&dir, "develop"), develop);
    assert_eq!(ops::git(&dir, &["branch", "--show-current"]).unwrap().trim(), "feature/login");
}

fn conflicting_repo(name: &str) -> PathBuf {
    let dir = temp_repo(name);
    commit_file(&dir, "a.txt", "top\nmiddle\nbottom\n", "初始");
    ops::create_branch(&dir, "other", "HEAD", true).unwrap();
    commit_file(&dir, "a.txt", "top\ntheirs\nbottom\n", "传入的修改");
    ops::checkout(&dir, "main").unwrap();
    commit_file(&dir, "a.txt", "top\nours\nbottom\n", "当前的修改");
    assert!(ops::merge(&dir, "other").is_err());
    dir
}

#[test]
fn resolve_conflict_by_chunks() {
    let dir = conflicting_repo("chunks");
    let f = conflict::load(&dir, "a.txt").unwrap();
    assert_eq!(f.conflicts, 1);
    assert!(f.utf8 && f.ours_exists && f.theirs_exists);
    conflict::resolve(&dir, "a.txt", &[conflict::Choice::OursTheirs]).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "top\nours\ntheirs\nbottom\n");
    // 已经标记为解决
    assert!(ops::git(&dir, &["diff", "--name-only", "--diff-filter=U"]).unwrap().trim().is_empty());
}

#[test]
fn resolve_conflict_whole_file() {
    let dir = conflicting_repo("side");
    conflict::resolve_side(&dir, "a.txt", true).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "top\ntheirs\nbottom\n");
    // 选择数量和冲突数量不一致时拒绝
    let dir = conflicting_repo("mismatch");
    assert!(conflict::resolve(&dir, "a.txt", &[]).is_err());
}

#[test]
fn history_follows_rename_and_blame() {
    let dir = temp_repo("history");
    commit_file(&dir, "old.txt", "a\nb\n", "第一版");
    ops::git(&dir, &["mv", "old.txt", "new.txt"]).unwrap();
    ops::commit(&dir, "改名", false).unwrap();
    commit_file(&dir, "new.txt", "a\nb\nc\n", "加一行");
    let h = history::file_history(&dir, "new.txt", 100).unwrap();
    assert_eq!(h.iter().map(|e| e.summary.as_str()).collect::<Vec<_>>(), ["加一行", "改名", "第一版"]);
    assert_eq!(h[1].old_path.as_deref(), Some("old.txt"));
    assert_eq!(h[2].path, "old.txt");

    std::fs::write(dir.join("new.txt"), "a\nb\nc\nd\n").unwrap();
    let b = history::blame(&dir, "new.txt", None).unwrap();
    assert_eq!(b.lines, ["a", "b", "c", "d"]);
    let who = |i: usize| &b.commits[b.line_commit[i] as usize];
    assert_eq!(who(0).summary, "第一版");
    assert_eq!(who(2).summary, "加一行");
    assert!(who(3).uncommitted);
    assert_eq!(who(0).author, "测试者");
}
