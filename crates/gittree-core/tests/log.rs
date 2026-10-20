use gittree_core::log::{Log, RefKind, open_gix, read_meta_parallel};
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "测试者")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "测试者")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn temp_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gittree-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    dir
}

fn commit(dir: &Path, msg: &str, ts: u32) {
    let date = format!("{ts} +0800");
    let out = Command::new("git")
        .args(["commit", "-q", "--allow-empty", "-m", msg])
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "测试者")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "测试者")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .output()
        .unwrap();
    assert!(out.status.success());
}

fn check_history(dir: &Path, log: &mut Log) {
    // 与 git 自己的 --date-order 输出一致
    let out = Command::new("git")
        .args(["log", "--all", "--date-order", "--format=%H"])
        .current_dir(dir)
        .output()
        .unwrap();
    let want: Vec<String> = String::from_utf8(out.stdout).unwrap().lines().map(str::to_string).collect();
    let got: Vec<String> = log.oids.iter().map(|o| o.to_string()).collect();
    assert_eq!(got.len(), want.len());
    // 子提交必须排在父提交前面
    for r in 0..log.len() {
        for &p in log.parents_of(r) {
            assert!(p as usize > r);
        }
    }
    let repo = open_gix(dir).unwrap();
    log.ensure_meta(&repo, 0..log.len());
    assert_eq!(log.meta_missing, 0);
}

#[test]
fn loads_branches_merges_and_ahead_behind() {
    let dir = temp_repo("basic");
    commit(&dir, "初始提交", 1_700_000_000);
    commit(&dir, "第二个提交", 1_700_000_100);
    git(&dir, &["checkout", "-q", "-b", "feature"]);
    commit(&dir, "功能 1", 1_700_000_200);
    commit(&dir, "功能 2", 1_700_000_300);
    git(&dir, &["checkout", "-q", "main"]);
    commit(&dir, "主线修复", 1_700_000_400);
    git(&dir, &["tag", "-a", "v1.0", "-m", "版本 1.0"]);
    git(&dir, &["merge", "-q", "--no-ff", "-m", "合并 feature", "feature"]);
    commit(&dir, "合并后", 1_700_000_600);
    // 用另一个分支模拟上游：HEAD~2 是“主线修复”，main 多出 合并后、合并提交、功能 1、功能 2 共 4 个
    git(&dir, &["update-ref", "refs/remotes/origin/main", "HEAD~2"]);
    git(&dir, &["config", "branch.main.remote", "origin"]);
    git(&dir, &["config", "branch.main.merge", "refs/heads/main"]);

    let mut log = Log::load(&dir).unwrap();
    assert_eq!(log.len(), 7);
    assert_eq!(log.head.branch.as_deref(), Some("main"));
    assert_eq!(log.head.row, Some(0));
    check_history(&dir, &mut log);

    let main = log.refs.iter().find(|r| r.name == "main").unwrap();
    assert!(main.is_head);
    assert_eq!(main.upstream.as_deref(), Some("refs/remotes/origin/main"));
    assert_eq!((main.ahead, main.behind), (4, 0));
    let tag = log.refs.iter().find(|r| r.kind == RefKind::Tag).unwrap();
    assert_eq!(tag.name, "v1.0");
    // 附注标签要解引用到它指向的提交
    let tag_row = tag.row.unwrap() as usize;
    assert_eq!(&*log.meta[tag_row].summary, "主线修复");
    // feature 和 main 互相比较：feature 不领先（已合并），落后 3 个
    let feature = log.refs.iter().find(|r| r.name == "feature").unwrap().row.unwrap();
    assert_eq!(log.ahead_behind(feature, 0), (0, 3));
    assert_eq!(log.author(0).unwrap().0.as_ref(), "测试者");
    // 合并提交有两个父提交，图里占两列
    assert_eq!(log.parents_of(1).len(), 2);
    assert_eq!(log.graph.max_lanes, 2);

    // 有 commit-graph 时结果相同，并且确实走了 commit-graph
    git(&dir, &["commit-graph", "write", "--reachable"]);
    commit(&dir, "commit-graph 之后的新提交", 1_700_000_700);
    let mut log2 = Log::load(&dir).unwrap();
    assert_eq!(log2.len(), 8);
    assert_eq!(log2.timings.from_objects, 1);
    assert_eq!(log2.timings.from_commit_graph, 7);
    let rows = log2.missing_meta_rows();
    assert!(!rows.is_empty());
    let metas = read_meta_parallel(&open_gix(&dir).unwrap().into_sync(), &log2.oids, &rows);
    log2.install_meta(metas);
    assert_eq!(log2.meta_missing, 0);
    assert_eq!(&*log2.meta[0].summary, "commit-graph 之后的新提交");
    check_history(&dir, &mut log2);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn empty_repo_loads() {
    let dir = temp_repo("empty");
    let log = Log::load(&dir).unwrap();
    assert!(log.is_empty());
    // 空仓库的 HEAD 指向还没有提交的 main 分支
    assert_eq!(log.head.branch.as_deref(), Some("main"));
    assert_eq!(log.head.oid, None);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn focus_shows_only_selected_branches() {
    let dir = temp_repo("focus");
    commit(&dir, "初始提交", 1_700_000_000);
    git(&dir, &["checkout", "-q", "-b", "feature"]);
    commit(&dir, "功能 1", 1_700_000_100);
    git(&dir, &["checkout", "-q", "-b", "other", "main"]);
    commit(&dir, "其他分支", 1_700_000_200);
    git(&dir, &["checkout", "-q", "main"]);
    commit(&dir, "主线", 1_700_000_300);

    let repo = open_gix(&dir).unwrap();
    assert_eq!(Log::load_repo(&repo, &[]).unwrap().len(), 4);

    // 只看当前分支（main）：看不到 feature 和 other 上的提交
    let mut only_head = Log::load_repo(&repo, &["HEAD".to_string()]).unwrap();
    assert_eq!(only_head.len(), 2);
    only_head.ensure_meta(&repo, 0..2);
    assert_eq!(&*only_head.meta[0].summary, "主线");
    // 分支列表仍然完整，只是不在图里的分支没有行号
    let feature = only_head.refs.iter().find(|r| r.name == "feature").unwrap();
    assert_eq!(feature.row, None);
    assert_eq!(only_head.head.row, Some(0));

    // 只看指定的两个分支
    let two = Log::load_repo(&repo, &["refs/heads/main".into(), "refs/heads/feature".into()]).unwrap();
    assert_eq!(two.len(), 3);
    // 不存在的分支被忽略
    let none = Log::load_repo(&repo, &["refs/heads/nope".into()]).unwrap();
    assert!(none.is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}
