use gittree_core::rebase::{self, Action, Step};
use gittree_core::ops;
use std::path::{Path, PathBuf};

fn temp_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gittree-rebase-{name}-{}", std::process::id()));
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

fn commit_file(dir: &Path, file: &str, msg: &str) {
    std::fs::write(dir.join(file), format!("{msg}\n")).unwrap();
    ops::stage_all(dir).unwrap();
    ops::commit(dir, msg, false).unwrap();
}

fn subjects(dir: &Path) -> Vec<String> {
    ops::git(dir, &["log", "--format=%s", "--reverse"]).unwrap().lines().map(String::from).collect()
}

fn step(oid: &str, action: Action, message: Option<&str>) -> Step {
    Step { oid: oid.to_string(), action, message: message.map(String::from) }
}

#[test]
fn reorder_squash_drop_and_reword() {
    let dir = temp_repo("all");
    let git_dir = dir.join(".git");
    for m in ["初始", "A", "B", "C", "D"] {
        commit_file(&dir, &format!("{m}.txt"), m);
    }
    let plan = rebase::plan(&dir, &ops::git(&dir, &["rev-parse", "HEAD~3"]).unwrap().trim().to_string()).unwrap();
    assert_eq!(plan.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(), ["A", "B", "C", "D"]);
    let oid = |s: &str| plan.iter().find(|c| c.summary == s).unwrap().oid.clone();

    // 新顺序：C、A（合并 D，改说明）、删掉 B；工作区里有未提交的改动也没关系
    std::fs::write(dir.join("初始.txt"), "未提交\n").unwrap();
    let steps = [
        step(&oid("C"), Action::Pick, Some("C 改过的说明\n\n正文")),
        step(&oid("A"), Action::Pick, Some("A 和 D 合在一起")),
        step(&oid("D"), Action::Squash, None),
        step(&oid("B"), Action::Drop, None),
    ];
    rebase::run(&dir, &git_dir, &oid("A"), &steps).unwrap();
    assert_eq!(subjects(&dir), ["初始", "C 改过的说明", "A 和 D 合在一起"]);
    assert_eq!(ops::git(&dir, &["log", "-1", "--skip=1", "--format=%b"]).unwrap().trim(), "正文");
    assert!(!dir.join("B.txt").exists());
    assert!(dir.join("D.txt").exists());
    // 自动 stash 的改动还在
    assert_eq!(std::fs::read_to_string(dir.join("初始.txt")).unwrap(), "未提交\n");
}

#[test]
fn root_rebase_and_validation() {
    let dir = temp_repo("root");
    let git_dir = dir.join(".git");
    commit_file(&dir, "a.txt", "第一个");
    commit_file(&dir, "b.txt", "第二个");
    let root = ops::git(&dir, &["rev-list", "--max-parents=0", "HEAD"]).unwrap().trim().to_string();
    let plan = rebase::plan(&dir, &root).unwrap();
    assert_eq!(plan.len(), 2);
    // 第一个不能是 squash
    assert!(rebase::run(&dir, &git_dir, &root, &[step(&plan[0].oid, Action::Squash, None), step(&plan[1].oid, Action::Pick, None)]).is_err());
    // 从根提交开始把两个合成一个
    rebase::run(&dir, &git_dir, &root, &[step(&plan[0].oid, Action::Pick, Some("合并后")), step(&plan[1].oid, Action::Squash, None)]).unwrap();
    assert_eq!(subjects(&dir), ["合并后"]);
}

#[test]
fn plan_rejects_commit_not_on_branch() {
    let dir = temp_repo("other");
    commit_file(&dir, "a.txt", "初始");
    ops::create_branch(&dir, "dev", "HEAD", true).unwrap();
    commit_file(&dir, "b.txt", "dev");
    let dev = ops::git(&dir, &["rev-parse", "HEAD"]).unwrap().trim().to_string();
    ops::checkout(&dir, "main").unwrap();
    assert!(rebase::plan(&dir, &dev).is_err());
}

#[test]
fn clone_and_merge_into() {
    let src = temp_repo("clone-src");
    commit_file(&src, "a.txt", "初始");
    ops::create_branch(&src, "dev", "HEAD", true).unwrap();
    commit_file(&src, "b.txt", "dev 的提交");
    ops::checkout(&src, "main").unwrap();
    // 在 dev 上把 main 合并进去（已经包含，快进也没有），再在 main 上合并 dev
    ops::merge_into(&src, "main", "dev").unwrap();
    assert!(src.join("b.txt").exists());

    let parent = std::env::temp_dir().join(format!("gittree-clone-parent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&parent);
    let url = format!("file://{}", src.display());
    let target = ops::clone(&parent, &url, "副本").unwrap();
    assert!(target.join("b.txt").exists());
    // 目标不是空文件夹时拒绝
    assert!(ops::clone(&parent, &url, "副本").is_err());
}
