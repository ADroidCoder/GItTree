use gittree_core::flow::{self, FinishOptions, Kind};
use gittree_core::ops::git;
use std::path::{Path, PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gittree-flow-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn repo(name: &str) -> PathBuf {
    let dir = temp_dir(name);
    git(&dir, &["init", "-q", "-b", "main"]).unwrap();
    git(&dir, &["config", "user.name", "测试者"]).unwrap();
    git(&dir, &["config", "user.email", "t@example.com"]).unwrap();
    git(&dir, &["config", "commit.gpgsign", "false"]).unwrap();
    git(&dir, &["config", "tag.gpgsign", "false"]).unwrap();
    write_commit(&dir, "a.txt", "1\n", "初始提交");
    dir
}

fn write_commit(dir: &Path, file: &str, content: &str, msg: &str) {
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", "-A"]).unwrap();
    git(dir, &["commit", "-q", "-m", msg]).unwrap();
}

fn current(dir: &Path) -> String {
    git(dir, &["branch", "--show-current"]).unwrap().trim().to_string()
}

fn contains(dir: &Path, commit: &str, branch: &str) -> bool {
    git(dir, &["merge-base", "--is-ancestor", commit, branch]).is_ok()
}

fn init(dir: &Path) {
    let mut cfg = flow::config(dir);
    assert!(!cfg.initialized);
    assert_eq!(cfg.master, "main");
    cfg.version_tag = "v".into();
    flow::init(dir, &cfg).unwrap();
    let cfg = flow::config(dir);
    assert!(cfg.initialized);
    assert_eq!((cfg.develop.as_str(), cfg.version_tag.as_str()), ("develop", "v"));
}

#[test]
fn feature_release_hotfix_lifecycle() {
    let dir = repo("lifecycle");
    let git_dir = dir.join(".git");
    init(&dir);

    // feature：从 develop 开始，完成后合并回 develop 并删除分支
    assert_eq!(flow::start(&dir, Kind::Feature, "login", None).unwrap(), "feature/login");
    assert_eq!(current(&dir), "feature/login");
    write_commit(&dir, "login.txt", "login\n", "登录功能");
    let feat = git(&dir, &["rev-parse", "HEAD"]).unwrap();
    flow::finish(&dir, &git_dir, Kind::Feature, "login", &FinishOptions::default()).unwrap();
    assert_eq!(current(&dir), "develop");
    assert!(contains(&dir, feat.trim(), "develop"));
    assert!(!contains(&dir, feat.trim(), "main"));
    assert!(git(&dir, &["rev-parse", "--verify", "-q", "refs/heads/feature/login"]).is_err());

    // 重名分支和非法名称要报错
    assert!(flow::start(&dir, Kind::Feature, "bad name", None).is_err());

    // release：合并到 main、打标签、合并回 develop
    flow::start(&dir, Kind::Release, "1.0.0", None).unwrap();
    write_commit(&dir, "version.txt", "1.0.0\n", "版本号 1.0.0");
    let out = flow::finish(
        &dir,
        &git_dir,
        Kind::Release,
        "1.0.0",
        &FinishOptions { tag_message: Some("第一个正式版".into()), ..Default::default() },
    )
    .unwrap();
    assert!(out.contains("v1.0.0"), "{out}");
    assert!(contains(&dir, "v1.0.0", "main"));
    assert!(contains(&dir, "v1.0.0", "develop"));
    assert!(contains(&dir, feat.trim(), "main"));
    let msg = git(&dir, &["tag", "-l", "--format=%(contents)", "v1.0.0"]).unwrap();
    assert_eq!(msg.trim(), "第一个正式版");
    // 已存在的版本号不能再开 release
    assert!(flow::start(&dir, Kind::Release, "1.0.0", None).is_err());

    // hotfix：从 main 开始，develop 上的新功能不能被带进来
    flow::start(&dir, Kind::Feature, "wip", None).unwrap();
    write_commit(&dir, "wip.txt", "wip\n", "开发中");
    let wip = git(&dir, &["rev-parse", "HEAD"]).unwrap();
    flow::finish(&dir, &git_dir, Kind::Feature, "wip", &FinishOptions::default()).unwrap();
    flow::start(&dir, Kind::Hotfix, "1.0.1", None).unwrap();
    assert!(!contains(&dir, wip.trim(), "HEAD"));
    write_commit(&dir, "a.txt", "1 fixed\n", "紧急修复");
    flow::finish(&dir, &git_dir, Kind::Hotfix, "1.0.1", &FinishOptions { keep_branch: true, ..Default::default() }).unwrap();
    assert!(contains(&dir, "v1.0.1", "main"));
    assert!(contains(&dir, "v1.0.1", "develop"));
    assert!(!contains(&dir, wip.trim(), "main"));
    // keep_branch 时分支保留
    assert!(git(&dir, &["rev-parse", "--verify", "-q", "refs/heads/hotfix/1.0.1"]).is_ok());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn finish_resumes_after_conflict() {
    let dir = repo("conflict");
    let git_dir = dir.join(".git");
    init(&dir);
    flow::start(&dir, Kind::Release, "2.0", None).unwrap();
    write_commit(&dir, "a.txt", "release\n", "release 改 a");
    // main 上也改了同一行（模拟期间有 hotfix），合并会冲突
    git(&dir, &["switch", "-q", "main"]).unwrap();
    write_commit(&dir, "a.txt", "main\n", "main 改 a");
    git(&dir, &["switch", "-q", "release/2.0"]).unwrap();

    let err = flow::finish(&dir, &git_dir, Kind::Release, "2.0", &FinishOptions::default()).unwrap_err();
    assert!(err.to_string().contains("冲突"), "{err}");
    assert_eq!(current(&dir), "main");
    // 再点一次完成：还没解决冲突，要提示
    let err = flow::finish(&dir, &git_dir, Kind::Release, "2.0", &FinishOptions::default()).unwrap_err();
    assert!(err.to_string().contains("没完成的合并"), "{err}");

    // 解决冲突并提交后再完成，从中断处继续
    std::fs::write(dir.join("a.txt"), "resolved\n").unwrap();
    git(&dir, &["add", "a.txt"]).unwrap();
    git(&dir, &["commit", "-q", "--no-edit"]).unwrap();
    let out = flow::finish(&dir, &git_dir, Kind::Release, "2.0", &FinishOptions::default()).unwrap();
    assert!(out.contains("跳过合并"), "{out}");
    assert!(contains(&dir, "v2.0", "main"));
    assert!(contains(&dir, "v2.0", "develop"));
    assert_eq!(current(&dir), "develop");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn finish_with_push_and_publish() {
    let remote = temp_dir("remote");
    git(&remote, &["init", "-q", "--bare", "-b", "main"]).unwrap();
    let dir = repo("push");
    let git_dir = dir.join(".git");
    git(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]).unwrap();
    git(&dir, &["push", "-q", "origin", "main"]).unwrap();
    init(&dir);

    flow::start(&dir, Kind::Feature, "share", None).unwrap();
    write_commit(&dir, "s.txt", "s\n", "共享功能");
    flow::publish(&dir, Kind::Feature, "share").unwrap();
    assert!(git(&remote, &["rev-parse", "--verify", "-q", "refs/heads/feature/share"]).is_ok());
    flow::finish(&dir, &git_dir, Kind::Feature, "share", &FinishOptions::default()).unwrap();

    flow::start(&dir, Kind::Release, "3.0", None).unwrap();
    flow::finish(&dir, &git_dir, Kind::Release, "3.0", &FinishOptions { push: true, ..Default::default() }).unwrap();
    let local_main = git(&dir, &["rev-parse", "main"]).unwrap();
    assert_eq!(git(&remote, &["rev-parse", "main"]).unwrap(), local_main);
    assert!(git(&remote, &["rev-parse", "--verify", "-q", "refs/tags/v3.0"]).is_ok());
    assert!(git(&remote, &["rev-parse", "--verify", "-q", "refs/heads/develop"]).is_ok());
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&remote).unwrap();
}

/// 和命令行 git flow 互通：用它初始化，我们读取；混着用它和我们来开始 / 完成分支。
/// 没安装 git-flow 时跳过。
#[test]
fn interoperates_with_git_flow_cli() {
    if git(&std::env::temp_dir(), &["flow", "version"]).is_err() {
        eprintln!("没有安装 git-flow，跳过");
        return;
    }
    let dir = repo("cli");
    let git_dir = dir.join(".git");
    // git flow init -d 使用默认值；这里把生产分支指定成 main
    git(&dir, &["config", "gitflow.branch.master", "main"]).unwrap();
    git(&dir, &["flow", "init", "-d"]).unwrap();

    let cfg = flow::config(&dir);
    assert!(cfg.initialized);
    assert_eq!(cfg.master, "main");
    assert_eq!(cfg.develop, "develop");
    assert_eq!(cfg.feature, "feature/");
    assert_eq!(cfg.release, "release/");

    // 我们开始 feature，命令行完成
    flow::start(&dir, Kind::Feature, "from-app", None).unwrap();
    write_commit(&dir, "x.txt", "x\n", "app 开始的功能");
    let feat = git(&dir, &["rev-parse", "HEAD"]).unwrap();
    git(&dir, &["flow", "feature", "finish", "from-app"]).unwrap();
    assert!(contains(&dir, feat.trim(), "develop"));

    // 命令行开始 release，我们完成
    git(&dir, &["flow", "release", "start", "4.0"]).unwrap();
    write_commit(&dir, "v.txt", "4.0\n", "版本 4.0");
    flow::finish(&dir, &git_dir, Kind::Release, "4.0", &FinishOptions::default()).unwrap();
    assert!(contains(&dir, "4.0", "main"));
    assert!(contains(&dir, "4.0", "develop"));

    // 我们重新保存设置不会改动命令行写入的其他配置
    git(&dir, &["config", "gitflow.prefix.support", "lts/"]).unwrap();
    flow::init(&dir, &flow::config(&dir)).unwrap();
    assert_eq!(git(&dir, &["config", "--get", "gitflow.prefix.support"]).unwrap().trim(), "lts/");
    std::fs::remove_dir_all(&dir).unwrap();
}
