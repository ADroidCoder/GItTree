//! Reflog、.gitignore、工作树、子模块、二分查错、签名、图片差异、PR 检出

use gittree_core::{bisect, diff, ops, reflog, submodule, worktree};
use std::path::{Path, PathBuf};

fn temp_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gittree-tools-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    ops::git(&dir, &["init", "-q", "-b", "main"]).unwrap();
    ops::git(&dir, &["config", "user.name", "测试者"]).unwrap();
    ops::git(&dir, &["config", "user.email", "t@example.com"]).unwrap();
    ops::git(&dir, &["config", "commit.gpgsign", "false"]).unwrap();
    ops::git(&dir, &["config", "core.autocrlf", "false"]).unwrap();
    dir
}

fn commit_file(dir: &Path, file: &str, content: &str, msg: &str) -> String {
    std::fs::write(dir.join(file), content).unwrap();
    ops::stage_all(dir).unwrap();
    ops::commit(dir, msg, false).unwrap();
    ops::git(dir, &["rev-parse", "HEAD"]).unwrap().trim().to_string()
}

#[test]
fn reflog_finds_commits_lost_by_reset() {
    let dir = temp_repo("reflog");
    assert!(reflog::reflog(&dir, None, 100).unwrap().is_empty());
    commit_file(&dir, "a.txt", "1\n", "第一个");
    let lost = commit_file(&dir, "a.txt", "2\n", "会丢的提交");
    ops::reset(&dir, "HEAD~1", "hard").unwrap();

    let entries = reflog::reflog(&dir, None, 100).unwrap();
    assert_eq!(entries[0].action, "reset");
    let found = entries.iter().find(|e| e.oid == lost).expect("reflog 里能找到被 reset 掉的提交");
    assert_eq!(found.summary, "会丢的提交");
    assert!(found.time > 0);

    // 从 reflog 里的提交新建分支就找回来了
    ops::create_branch(&dir, "rescue", &lost, false).unwrap();
    assert_eq!(ops::git(&dir, &["rev-parse", "rescue"]).unwrap().trim(), lost);
}

#[test]
fn ignore_untracked_files() {
    let dir = temp_repo("ignore");
    commit_file(&dir, "a.txt", "1\n", "初始");
    std::fs::write(dir.join(".gitignore"), "target").unwrap(); // 末尾没有换行
    std::fs::create_dir_all(dir.join("logs")).unwrap();
    std::fs::write(dir.join("logs/x.log"), "").unwrap();
    std::fs::write(dir.join("secret [1].txt"), "").unwrap();
    std::fs::write(dir.join("keep.txt"), "").unwrap();

    ops::ignore(&dir, &["*.log".into()]).unwrap();
    ops::ignore(&dir, &["*.log".into(), ops::ignore_pattern_for("secret [1].txt")]).unwrap(); // *.log 不会重复加
    assert_eq!(std::fs::read_to_string(dir.join(".gitignore")).unwrap(), "target\n*.log\n/secret \\[1].txt\n");

    let st = ops::status(&dir, &dir.join(".git")).unwrap();
    let names: Vec<_> = st.unstaged.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(names, [".gitignore", "keep.txt"]);
}

#[test]
fn worktree_add_list_remove() {
    let dir = temp_repo("wt");
    commit_file(&dir, "a.txt", "1\n", "初始");
    let wt_path = dir.with_file_name(format!("{}-feature", dir.file_name().unwrap().to_string_lossy()));
    let _ = std::fs::remove_dir_all(&wt_path);
    worktree::add(&dir, &wt_path.to_string_lossy(), "feature", true, Some("main")).unwrap();

    let list = worktree::list(&dir).unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].main && list[0].current);
    assert_eq!(list[1].branch.as_deref(), Some("feature"));
    assert!(!list[1].current);
    // 在工作树里看，它自己才是当前的
    assert!(worktree::list(&wt_path).unwrap()[1].current);

    // 分支已经在别的工作树检出时不能再切换过去，报错里有路径
    let err = ops::checkout(&dir, "feature").unwrap_err().to_string();
    assert!(err.contains("worktree") || err.contains("checked out"), "{err}");

    // 有改动时普通删除失败，强制删除可以
    std::fs::write(wt_path.join("a.txt"), "changed\n").unwrap();
    assert!(worktree::remove(&dir, &wt_path.to_string_lossy(), false).is_err());
    worktree::remove(&dir, &wt_path.to_string_lossy(), true).unwrap();
    assert_eq!(worktree::list(&dir).unwrap().len(), 1);
}

#[test]
fn submodule_status_and_update() {
    let lib = temp_repo("sub-lib");
    commit_file(&lib, "lib.txt", "v1\n", "库 v1");
    let app = temp_repo("sub-app");
    commit_file(&app, "app.txt", "1\n", "初始");
    let url = format!("file://{}", lib.display());
    ops::git(&app, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", &url, "libs/lib"]).unwrap();
    ops::commit(&app, "加子模块", false).unwrap();

    let subs = submodule::list(&app).unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!((subs[0].path.as_str(), subs[0].name.as_str(), subs[0].state.as_str()), ("libs/lib", "libs/lib", "ok"));
    // Windows 上 git 会把地址里的 \ 存成 /
    assert_eq!(subs[0].url.replace('\\', "/"), url.replace('\\', "/"));

    // 克隆下来的仓库里子模块还没初始化
    let parent = app.parent().unwrap();
    let clone_name = format!("gittree-tools-sub-clone-{}", std::process::id());
    let _ = std::fs::remove_dir_all(parent.join(&clone_name));
    ops::git(parent, &["clone", "-q", &format!("file://{}", app.display()), &clone_name]).unwrap();
    let clone = parent.join(&clone_name);
    assert_eq!(submodule::list(&clone).unwrap()[0].state, "uninitialized");
    ops::git(&clone, &["config", "protocol.file.allow", "always"]).unwrap();
    // git submodule 读的是全局 / 命令行配置，测试里用环境变量放开 file:// 协议
    let out = std::process::Command::new("git")
        .current_dir(&clone)
        .args(["-c", "protocol.file.allow=always", "submodule", "update", "--init", "--recursive"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(submodule::list(&clone).unwrap()[0].state, "ok");
    assert_eq!(std::fs::read_to_string(clone.join("libs/lib/lib.txt")).unwrap(), "v1\n");

    // 子模块里检出了别的提交
    commit_file(&clone.join("libs/lib"), "lib.txt", "local\n", "本地改");
    assert_eq!(submodule::list(&clone).unwrap()[0].state, "modified");
    // update 把它恢复到父仓库记录的提交
    submodule::update(&clone, Some("libs/lib")).unwrap();
    assert_eq!(submodule::list(&clone).unwrap()[0].state, "ok");

    assert!(submodule::list(&lib).unwrap().is_empty());
}

#[test]
fn bisect_finds_first_bad_commit() {
    let dir = temp_repo("bisect");
    let mut oids = Vec::new();
    for i in 0..8 {
        // 从第 5 个提交开始 “有 bug”
        let content = if i >= 5 { format!("{i} bug\n") } else { format!("{i}\n") };
        oids.push(commit_file(&dir, "a.txt", &content, &format!("提交 {i}")));
    }
    let git_dir = dir.join(".git");
    assert!(!bisect::state(&dir, &git_dir).unwrap().active);

    let msg = bisect::start(&dir, &oids[7], &oids[0]).unwrap();
    assert!(!msg.is_empty());
    let mut st = bisect::state(&dir, &git_dir).unwrap();
    assert!(st.active);
    assert_eq!(st.bad.as_deref(), Some(oids[7].as_str()));
    assert_eq!(st.good, [oids[0].clone()]);
    assert_eq!(st.remaining, Some(7));

    let mut rounds = 0;
    while st.found.is_none() {
        rounds += 1;
        assert!(rounds < 6, "二分查错没有结束");
        let content = std::fs::read_to_string(dir.join("a.txt")).unwrap();
        bisect::mark(&dir, if content.contains("bug") { "bad" } else { "good" }, None).unwrap();
        st = bisect::state(&dir, &git_dir).unwrap();
    }
    assert_eq!(st.found.as_deref(), Some(oids[5].as_str()));

    bisect::reset(&dir).unwrap();
    assert!(!bisect::state(&dir, &git_dir).unwrap().active);
    assert_eq!(ops::git(&dir, &["symbolic-ref", "--short", "HEAD"]).unwrap().trim(), "main");
}

#[test]
fn unsigned_commit_has_no_signature() {
    let dir = temp_repo("sig");
    let oid = commit_file(&dir, "a.txt", "1\n", "初始");
    assert!(ops::signature(&dir, &oid).unwrap().is_none());
}

#[test]
fn image_pair_reads_both_sides() {
    let dir = temp_repo("image");
    // 两个“图片”：只比较字节，不需要真的是图片
    let v1 = [0x89u8, b'P', b'N', b'G', 1, 2, 3];
    let v2 = [0x89u8, b'P', b'N', b'G', 9, 9];
    std::fs::write(dir.join("logo.png"), v1).unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "图片 v1", false).unwrap();
    std::fs::write(dir.join("logo.png"), v2).unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "图片 v2", false).unwrap();
    let head = ops::git(&dir, &["rev-parse", "HEAD"]).unwrap().trim().to_string();
    let repo = git2::Repository::open(&dir).unwrap();

    let p = diff::image_pair(&repo, "commit", Some(&head), None, None, false, "logo.png", None).unwrap();
    assert_eq!(p.old.as_ref().unwrap().size, 7);
    assert_eq!(p.new.as_ref().unwrap().data.as_deref(), Some("iVBORwkJ"));

    // 工作区：新增的文件只有新的一边
    std::fs::write(dir.join("new.png"), [1u8, 2, 3]).unwrap();
    let p = diff::image_pair(&repo, "workdir", None, None, None, false, "new.png", None).unwrap();
    assert!(p.old.is_none());
    assert_eq!(p.new.unwrap().size, 3);
    // 暂存区相对 HEAD
    std::fs::write(dir.join("logo.png"), [5u8]).unwrap();
    ops::stage_all(&dir).unwrap();
    let p = diff::image_pair(&repo, "workdir", None, None, None, true, "logo.png", None).unwrap();
    assert_eq!((p.old.unwrap().size, p.new.unwrap().size), (6, 1));
}

#[test]
fn checkout_pull_request_ref() {
    let remote = temp_repo("pr-remote");
    ops::git(&remote, &["config", "receive.denyCurrentBranch", "ignore"]).unwrap();
    let base = commit_file(&remote, "a.txt", "1\n", "初始");
    let pr_head = commit_file(&remote, "a.txt", "2\n", "PR 的提交");
    // 模拟托管平台为 PR 准备的引用
    ops::git(&remote, &["update-ref", "refs/pull/12/head", &pr_head]).unwrap();
    ops::git(&remote, &["reset", "-q", "--hard", &base]).unwrap();

    let local = temp_repo("pr-local");
    ops::git(&local, &["remote", "add", "origin", &format!("file://{}", remote.display())]).unwrap();
    ops::git(&local, &["fetch", "-q", "origin"]).unwrap();
    ops::git(&local, &["switch", "-q", "-c", "main", "origin/main"]).unwrap();

    ops::checkout_pull(&local, "origin", "refs/pull/12/head", "pr/12").unwrap();
    assert_eq!(ops::git(&local, &["symbolic-ref", "--short", "HEAD"]).unwrap().trim(), "pr/12");
    assert_eq!(ops::git(&local, &["rev-parse", "HEAD"]).unwrap().trim(), pr_head);

    // PR 有了新提交，再检出一次（已经在这个分支上）会更新
    ops::git(&remote, &["switch", "-q", "--detach", &pr_head]).unwrap();
    let newer = commit_file(&remote, "a.txt", "3\n", "PR 更新");
    ops::git(&remote, &["update-ref", "refs/pull/12/head", &newer]).unwrap();
    ops::checkout_pull(&local, "origin", "refs/pull/12/head", "pr/12").unwrap();
    assert_eq!(ops::git(&local, &["rev-parse", "HEAD"]).unwrap().trim(), newer);
}

#[test]
fn pull_with_stash_keeps_local_changes() {
    let remote = temp_repo("pull-stash-remote");
    ops::git(&remote, &["config", "receive.denyCurrentBranch", "ignore"]).unwrap();
    commit_file(&remote, "a.txt", "1\n2\n3\n4\n5\n6\n", "初始");

    let local = temp_repo("pull-stash-local");
    ops::git(&local, &["remote", "add", "origin", &format!("file://{}", remote.display())]).unwrap();
    ops::git(&local, &["fetch", "-q", "origin"]).unwrap();
    ops::git(&local, &["switch", "-q", "-c", "main", "--track", "origin/main"]).unwrap();
    ops::git(&local, &["config", "pull.rebase", "false"]).unwrap();

    commit_file(&remote, "a.txt", "1 remote\n2\n3\n4\n5\n6\n", "远程改了第一行");
    std::fs::write(local.join("a.txt"), "1\n2\n3\n4\n5\n6 local\n").unwrap();

    // 普通拉取被本地改动挡住，报错里有界面用来识别的关键字
    let err = ops::pull(&local).unwrap_err().to_string();
    assert!(err.contains("would be overwritten by merge") || err.contains("commit your changes or stash them"), "{err}");

    // 先 stash 再拉取，最后恢复：两边的改动都在
    ops::with_stash(&local, "拉取", true, || ops::pull(&local)).unwrap();
    assert_eq!(std::fs::read_to_string(local.join("a.txt")).unwrap(), "1 remote\n2\n3\n4\n5\n6 local\n");
    assert!(ops::stash_list(&local).unwrap().is_empty());
}

#[test]
fn push_branch_created_from_another_remote_branch() {
    let remote = temp_repo("push-new-remote");
    ops::git(&remote, &["config", "receive.denyCurrentBranch", "ignore"]).unwrap();
    commit_file(&remote, "a.txt", "1\n", "初始");
    ops::git(&remote, &["branch", "develop"]).unwrap();

    let local = temp_repo("push-new-local");
    ops::git(&local, &["remote", "add", "origin", &format!("file://{}", remote.display())]).unwrap();
    ops::git(&local, &["fetch", "-q", "origin"]).unwrap();

    // 界面上新建的分支不跟踪起点
    ops::create_branch(&local, "feature/a", "origin/develop", true).unwrap();
    assert!(ops::git(&local, &["rev-parse", "--abbrev-ref", "@{u}"]).is_err());
    commit_file(&local, "b.txt", "b\n", "功能 a");
    ops::push(&local, false).unwrap();
    assert_eq!(ops::git(&local, &["rev-parse", "--abbrev-ref", "@{u}"]).unwrap().trim(), "origin/feature/a");

    // 命令行里建的、跟踪着 origin/develop 的分支：推到同名的远程分支，develop 不受影响
    let develop = ops::git(&remote, &["rev-parse", "develop"]).unwrap();
    ops::git(&local, &["switch", "-q", "-c", "feature/b", "--track", "origin/develop"]).unwrap();
    commit_file(&local, "c.txt", "c\n", "功能 b");
    ops::push(&local, false).unwrap();
    assert_eq!(ops::git(&remote, &["rev-parse", "develop"]).unwrap(), develop);
    assert_eq!(ops::git(&remote, &["rev-parse", "feature/b"]).unwrap(), ops::git(&local, &["rev-parse", "HEAD"]).unwrap());
    assert_eq!(ops::git(&local, &["rev-parse", "--abbrev-ref", "@{u}"]).unwrap().trim(), "origin/feature/b");

    // 已经正常跟踪同名分支时照常推送
    commit_file(&local, "d.txt", "d\n", "再改");
    ops::push(&local, false).unwrap();
    assert_eq!(ops::git(&remote, &["rev-parse", "feature/b"]).unwrap(), ops::git(&local, &["rev-parse", "HEAD"]).unwrap());
}

#[test]
fn merge_modes() {
    use ops::MergeMode;
    let dir = temp_repo("merge-modes");
    let base = commit_file(&dir, "a.txt", "1\n", "初始");
    ops::git(&dir, &["switch", "-q", "-c", "feat"]).unwrap();
    commit_file(&dir, "b.txt", "b\n", "功能 1");
    let feat = commit_file(&dir, "c.txt", "c\n", "功能 2");
    ops::checkout(&dir, "main").unwrap();
    let parents = |r: &str| ops::git(&dir, &["rev-list", "--parents", "-n1", r]).unwrap().split_whitespace().count() - 1;

    // 可以快进时 --no-ff 也会生成合并提交
    ops::merge_with(&dir, "feat", MergeMode::NoFf, None).unwrap();
    assert_eq!(parents("HEAD"), 2);
    ops::reset(&dir, &base, "hard").unwrap();

    // 只快进
    ops::merge_with(&dir, "feat", MergeMode::FfOnly, None).unwrap();
    assert_eq!(ops::git(&dir, &["rev-parse", "HEAD"]).unwrap().trim(), feat);
    ops::reset(&dir, &base, "hard").unwrap();

    // 分叉后只快进会失败，并给出说明
    commit_file(&dir, "d.txt", "d\n", "主线");
    let err = ops::merge_with(&dir, "feat", MergeMode::FfOnly, None).unwrap_err().to_string();
    assert!(err.contains("快进") || err.contains("fast-forward"), "{err}");

    // 压缩成一个普通提交
    ops::merge_with(&dir, "feat", MergeMode::Squash, Some("合入功能")).unwrap();
    assert_eq!(parents("HEAD"), 1);
    assert_eq!(ops::git(&dir, &["log", "-1", "--format=%s"]).unwrap().trim(), "合入功能");
    assert!(dir.join("b.txt").exists() && dir.join("c.txt").exists());
    assert!(ops::git(&dir, &["status", "--porcelain"]).unwrap().trim().is_empty());

    // 已经合过了：不会产生空提交
    let head = ops::git(&dir, &["rev-parse", "HEAD"]).unwrap();
    ops::merge_with(&dir, "feat", MergeMode::Squash, None).unwrap();
    assert_eq!(ops::git(&dir, &["rev-parse", "HEAD"]).unwrap(), head);
}

#[test]
fn cherry_pick_and_revert_many() {
    let dir = temp_repo("pick-many");
    commit_file(&dir, "a.txt", "1\n", "初始");
    ops::git(&dir, &["switch", "-q", "-c", "feat"]).unwrap();
    let b = commit_file(&dir, "b.txt", "b\n", "功能 1");
    let c = commit_file(&dir, "c.txt", "c\n", "功能 2");
    ops::checkout(&dir, "main").unwrap();
    let log = || ops::git(&dir, &["log", "--format=%s"]).unwrap();

    // 按给出的顺序（旧的在前）依次拣选
    ops::cherry_pick(&dir, &[b.clone(), c.clone()]).unwrap();
    assert_eq!(log(), "功能 2\n功能 1\n初始\n");

    // 还原时新的在前，每个提交一个还原提交
    let picked: Vec<String> = ops::git(&dir, &["rev-list", "-n2", "HEAD"]).unwrap().lines().map(String::from).collect();
    ops::revert(&dir, &picked).unwrap();
    assert!(!dir.join("b.txt").exists() && !dir.join("c.txt").exists());
    assert_eq!(log().lines().count(), 5);
}
