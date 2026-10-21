use gittree_core::{diff, ops};
use std::path::PathBuf;

fn temp_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gittree-ops-{name}-{}", std::process::id()));
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

#[test]
fn stage_single_hunk_keeps_other_hunk_unstaged() {
    let dir = temp_repo("hunk");
    // 中间夹一段 GBK 编码的内容（“中文”= D6 D0 CE C4），确保补丁按原始字节生成
    let content = |changed: bool| {
        let mut out: Vec<u8> = Vec::new();
        for i in 1..=30 {
            let suffix = if changed && (i == 2 || i == 28) { " changed" } else { "" };
            out.extend_from_slice(format!("line {i}{suffix}\n").as_bytes());
            if i == 15 {
                out.extend_from_slice(b"\xD6\xD0\xCE\xC4\n");
            }
        }
        out
    };
    std::fs::write(dir.join("a.txt"), content(false)).unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "初始", false).unwrap();
    let changed = content(true);
    std::fs::write(dir.join("a.txt"), &changed).unwrap();

    let repo = git2::Repository::open(&dir).unwrap();
    let d = diff::workdir_file_diff(&repo, "a.txt", false).unwrap().unwrap();
    assert_eq!(d.hunks.len(), 2);

    // 只暂存第一个代码块
    let patch = diff::hunk_patch(&repo, "a.txt", false, &d.hunks[0].header).unwrap().unwrap();
    ops::apply_patch(&dir, &patch, true, false).unwrap();
    let staged = diff::workdir_file_diff(&repo, "a.txt", true).unwrap().unwrap();
    let unstaged = diff::workdir_file_diff(&repo, "a.txt", false).unwrap().unwrap();
    assert_eq!(staged.hunks.len(), 1);
    assert_eq!(unstaged.hunks.len(), 1);
    assert!(staged.hunks[0].lines.iter().any(|l| l.text == "line 2 changed"));
    assert!(unstaged.hunks[0].lines.iter().any(|l| l.text == "line 28 changed"));

    // 再把它从暂存区撤回
    let patch = diff::hunk_patch(&repo, "a.txt", true, &staged.hunks[0].header).unwrap().unwrap();
    ops::apply_patch(&dir, &patch, true, true).unwrap();
    assert!(diff::workdir_file_diff(&repo, "a.txt", true).unwrap().is_none());

    // 工作区文件一个字节都没被改动
    assert_eq!(std::fs::read(dir.join("a.txt")).unwrap(), changed);

    let st = ops::status(&dir, &dir.join(".git")).unwrap();
    assert_eq!(st.unstaged.len(), 1);
    assert!(st.staged.is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn commit_details_lists_files_with_stats() {
    let dir = temp_repo("details");
    std::fs::write(dir.join("a.txt"), "1\n2\n3\n").unwrap();
    std::fs::write(dir.join("old.txt"), "这是一个会被重命名的文件\n内容足够长才能被识别为重命名\n第三行\n").unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "初始", false).unwrap();
    std::fs::write(dir.join("a.txt"), "1\n二\n3\n4\n").unwrap();
    ops::git(&dir, &["mv", "old.txt", "new.txt"]).unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "修改\n\n详细说明", false).unwrap();

    let repo = git2::Repository::open(&dir).unwrap();
    let head = repo.head().unwrap().target().unwrap().to_string();
    let d = diff::commit_details(&repo, &head).unwrap();
    assert_eq!(d.message, "修改\n\n详细说明\n");
    assert_eq!(d.author.name, "测试者");
    let a = d.files.iter().find(|f| f.path == "a.txt").unwrap();
    assert_eq!((a.status, a.additions, a.deletions), ('M', 2, 1));
    let r = d.files.iter().find(|f| f.path == "new.txt").unwrap();
    assert_eq!(r.status, 'R');
    assert_eq!(r.old_path.as_deref(), Some("old.txt"));
    let fd = diff::commit_file_diff(&repo, &head, "a.txt", None).unwrap().unwrap();
    let kinds: String = fd.hunks[0].lines.iter().map(|l| l.kind).collect();
    assert_eq!(kinds, " -+ +");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stash_selected_files_only() {
    let dir = temp_repo("stash");
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    std::fs::write(dir.join("b.txt"), "b\n").unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "初始", false).unwrap();
    std::fs::write(dir.join("a.txt"), "a changed\n").unwrap();
    std::fs::write(dir.join("b.txt"), "b changed\n").unwrap();
    std::fs::write(dir.join("新文件.txt"), "new\n").unwrap();

    // 只 stash a.txt 和未跟踪的新文件，b.txt 留在工作区
    ops::stash_push(&dir, Some("部分 stash"), &["a.txt".into(), "新文件.txt".into()]).unwrap();
    let st = ops::status(&dir, &dir.join(".git")).unwrap();
    let left: Vec<_> = st.unstaged.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(left, ["b.txt"]);
    let list = ops::stash_list(&dir).unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].message.contains("部分 stash"));

    // stash 的内容：已跟踪文件看 stash 本身，未跟踪文件在第三个父提交里
    let repo = git2::Repository::open(&dir).unwrap();
    let d = diff::commit_details(&repo, &list[0].oid).unwrap();
    assert_eq!(d.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["a.txt"]);
    assert_eq!(d.parents.len(), 3);
    let untracked = diff::commit_details(&repo, &d.parents[2]).unwrap();
    assert_eq!(untracked.files[0].path, "新文件.txt");
    assert_eq!(untracked.files[0].status, 'A');

    ops::stash_action(&dir, "pop", 0).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "a changed\n");
    assert!(dir.join("新文件.txt").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn fetch_streams_progress_and_can_cancel() {
    use std::sync::{Arc, Mutex};
    let remote = temp_repo("progress-remote");
    for i in 0..30 {
        std::fs::write(remote.join(format!("f{i}.txt")), format!("{i}\n").repeat(200)).unwrap();
        ops::stage_all(&remote).unwrap();
        ops::commit(&remote, &format!("提交 {i}"), false).unwrap();
    }
    let local = temp_repo("progress-local");
    let url = format!("file://{}", remote.display());
    ops::git(&local, &["remote", "add", "origin", &url]).unwrap();

    let lines = Arc::new(Mutex::new(Vec::<ops::ProgressLine>::new()));
    let sink = lines.clone();
    let p = ops::Progress::new(move |l| sink.lock().unwrap().push(l));
    ops::with_progress(p, || ops::fetch(&local)).unwrap();
    let lines = lines.lock().unwrap();
    assert!(lines[0].text.starts_with("$ git fetch"), "{lines:?}");
    // file:// 协议也会输出对象进度，并且是用 \r 刷新的
    assert!(lines.iter().any(|l| l.text.contains("objects:")), "{lines:?}");
    assert!(lines.iter().any(|l| l.transient), "{lines:?}");
    assert!(ops::git(&local, &["rev-parse", "origin/main"]).is_ok());

    // 已取消的进度上下文不再执行 git
    let p = ops::Progress::new(|_| {});
    p.cancel();
    let err = ops::with_progress(p, || ops::fetch(&local)).unwrap_err();
    assert_eq!(err.to_string(), "已取消");
}

#[test]
fn rename_branch_local_and_remote() {
    let remote = temp_repo("rename-remote");
    ops::git(&remote, &["config", "receive.denyCurrentBranch", "ignore"]).unwrap();
    std::fs::write(remote.join("a.txt"), "a\n").unwrap();
    ops::stage_all(&remote).unwrap();
    ops::commit(&remote, "初始", false).unwrap();

    let local = temp_repo("rename-local");
    ops::git(&local, &["remote", "add", "origin", &format!("file://{}", remote.display())]).unwrap();
    ops::git(&local, &["fetch", "origin"]).unwrap();
    ops::git(&local, &["switch", "-c", "feture/login", "origin/main"]).unwrap();
    ops::git(&local, &["push", "-u", "origin", "feture/login"]).unwrap();

    // 只改本地：远程的旧名字还在，上游配置跟着改名后的分支走
    ops::rename_branch(&local, "feture/login", "feature/login-tmp", false).unwrap();
    assert!(ops::git(&local, &["rev-parse", "--verify", "feature/login-tmp"]).is_ok());
    assert!(ops::git(&remote, &["rev-parse", "--verify", "refs/heads/feture/login"]).is_ok());
    let up = ops::git(&local, &["rev-parse", "--abbrev-ref", "feature/login-tmp@{u}"]).unwrap();
    assert_eq!(up.trim(), "origin/feture/login");

    // 连远程一起改：远程只剩新名字，上游指向新名字
    ops::rename_branch(&local, "feature/login-tmp", "feature/login", true).unwrap();
    assert!(ops::git(&remote, &["rev-parse", "--verify", "refs/heads/feature/login"]).is_ok());
    assert!(ops::git(&remote, &["rev-parse", "--verify", "refs/heads/feture/login"]).is_err());
    let up = ops::git(&local, &["rev-parse", "--abbrev-ref", "feature/login@{u}"]).unwrap();
    assert_eq!(up.trim(), "origin/feature/login");

    // 新名字已经存在时报错，原分支不受影响
    ops::git(&local, &["branch", "other"]).unwrap();
    assert!(ops::rename_branch(&local, "other", "feature/login", false).is_err());
    assert!(ops::git(&local, &["rev-parse", "--verify", "other"]).is_ok());
}

#[test]
fn push_rejected_then_force_with_lease() {
    let remote = temp_repo("force-remote");
    ops::git(&remote, &["config", "receive.denyCurrentBranch", "ignore"]).unwrap();
    std::fs::write(remote.join("a.txt"), "a\n").unwrap();
    ops::stage_all(&remote).unwrap();
    ops::commit(&remote, "初始", false).unwrap();

    let local = temp_repo("force-local");
    ops::git(&local, &["remote", "add", "origin", &format!("file://{}", remote.display())]).unwrap();
    ops::git(&local, &["fetch", "origin"]).unwrap();
    ops::git(&local, &["switch", "--no-track", "-c", "dev", "origin/main"]).unwrap();
    std::fs::write(local.join("b.txt"), "b\n").unwrap();
    ops::stage_all(&local).unwrap();
    ops::commit(&local, "第一版", false).unwrap();
    ops::push(&local, false).unwrap();

    // 修改已经推送的提交后，普通推送被拒绝，报错里有界面用来识别的关键字
    ops::commit(&local, "第二版", true).unwrap();
    let err = ops::push(&local, false).unwrap_err().to_string();
    assert!(err.contains("[rejected]") || err.contains("non-fast-forward"), "{err}");

    // 强制推送成功，远程变成本地的提交
    ops::push(&local, true).unwrap();
    let head = ops::git(&local, &["rev-parse", "HEAD"]).unwrap();
    let remote_head = ops::git(&remote, &["rev-parse", "refs/heads/dev"]).unwrap();
    assert_eq!(head, remote_head);
}

#[test]
fn checkout_with_stash_carries_or_keeps_changes() {
    let dir = temp_repo("with-stash");
    std::fs::write(dir.join("f.txt"), "1\n2\n3\n4\n5\n6\n7\n8\n").unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "初始", false).unwrap();
    ops::git(&dir, &["switch", "-c", "other"]).unwrap();
    std::fs::write(dir.join("f.txt"), "1 other\n2\n3\n4\n5\n6\n7\n8\n").unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "other 改了第一行", false).unwrap();
    ops::git(&dir, &["switch", "main"]).unwrap();
    let stash_count = || ops::git(&dir, &["stash", "list"]).unwrap().lines().count();

    // 改了最后一行：直接切换会被拒绝
    std::fs::write(dir.join("f.txt"), "1\n2\n3\n4\n5\n6\n7\n8 mine\n").unwrap();
    std::fs::write(dir.join("new.txt"), "untracked\n").unwrap();
    let err = ops::checkout(&dir, "other").unwrap_err().to_string();
    assert!(err.contains("would be overwritten"), "{err}");

    // stash → 切换 → pop：改动带到了 other 上，两边的修改合在一起，stash 没有留下
    ops::with_stash(&dir, "切换到 other", true, || ops::checkout(&dir, "other").map(|_| String::new())).unwrap();
    assert_eq!(ops::git(&dir, &["branch", "--show-current"]).unwrap().trim(), "other");
    assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "1 other\n2\n3\n4\n5\n6\n7\n8 mine\n");
    assert!(dir.join("new.txt").exists());
    assert_eq!(stash_count(), 0);

    // 不恢复：切换后工作区是干净的，改动留在 stash 里
    ops::with_stash(&dir, "切换到 main", false, || ops::checkout(&dir, "main").map(|_| String::new())).unwrap();
    assert_eq!(ops::git(&dir, &["status", "--porcelain"]).unwrap().trim(), "");
    assert_eq!(stash_count(), 1);
    ops::git(&dir, &["stash", "drop"]).unwrap();

    // 切换本身失败：改动原样恢复，stash 不留下
    std::fs::write(dir.join("f.txt"), "changed\n").unwrap();
    assert!(ops::with_stash(&dir, "切换到 nope", true, || ops::checkout(&dir, "nope").map(|_| String::new())).is_err());
    assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "changed\n");
    assert_eq!(stash_count(), 0);

    // 恢复时冲突：报错，冲突留在工作区，原来的改动还在 stash 里
    std::fs::write(dir.join("f.txt"), "1 mine\n2\n3\n4\n5\n6\n7\n8\n").unwrap();
    let err = ops::with_stash(&dir, "切换到 other", true, || ops::checkout(&dir, "other").map(|_| String::new()))
        .unwrap_err()
        .to_string();
    assert!(err.contains("冲突"), "{err}");
    assert_eq!(ops::git(&dir, &["branch", "--show-current"]).unwrap().trim(), "other");
    assert_eq!(stash_count(), 1);
    assert!(std::fs::read_to_string(dir.join("f.txt")).unwrap().contains("<<<<<<<"));
}

#[test]
fn manage_remotes_and_push_tags() {
    let remote = temp_repo("tags-remote");
    let local = temp_repo("tags-local");
    std::fs::write(local.join("a.txt"), "a\n").unwrap();
    ops::stage_all(&local).unwrap();
    ops::commit(&local, "初始", false).unwrap();

    let url = format!("file://{}", remote.display());
    ops::add_remote(&local, "origin", &url).unwrap();
    ops::add_remote(&local, "upstream", "git@gitee.com:someone/repo.git").unwrap();
    ops::git(&local, &["remote", "set-url", "--push", "upstream", "git@gitee.com:me/repo.git"]).unwrap();
    let list = ops::remotes(&local).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0], ops::Remote { name: "origin".into(), url: url.clone(), push_url: None });
    assert_eq!(list[1].url, "git@gitee.com:someone/repo.git");
    assert_eq!(list[1].push_url.as_deref(), Some("git@gitee.com:me/repo.git"));

    ops::rename_remote(&local, "upstream", "fork").unwrap();
    ops::set_remote_url(&local, "fork", "https://gitee.com/someone/repo.git").unwrap();
    let fork = ops::remotes(&local).unwrap().into_iter().find(|r| r.name == "fork").unwrap();
    assert_eq!(fork.url, "https://gitee.com/someone/repo.git");
    ops::remove_remote(&local, "fork").unwrap();
    assert_eq!(ops::remotes(&local).unwrap().len(), 1);

    // 推送单个标签、所有标签，删除远程标签
    ops::git(&local, &["tag", "v1.0"]).unwrap();
    ops::git(&local, &["tag", "v1.1"]).unwrap();
    ops::push_tag(&local, "origin", Some("v1.0")).unwrap();
    let tags = || ops::git(&remote, &["tag"]).unwrap();
    assert_eq!(tags().trim(), "v1.0");
    ops::push_tag(&local, "origin", None).unwrap();
    assert_eq!(tags().lines().count(), 2);
    ops::delete_remote_tag(&local, "origin", "v1.0").unwrap();
    assert_eq!(tags().trim(), "v1.1");
}

#[test]
fn stage_unstage_and_discard_selected_lines() {
    use gittree_core::{diff, git2};
    let dir = temp_repo("lines");
    std::fs::write(dir.join("f.txt"), "a\nb\nc\nd\n").unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "初始", false).unwrap();
    // 一个代码块里：改了 b、删了 c、加了两行
    std::fs::write(dir.join("f.txt"), "a\nB\nd\nx\ny\n").unwrap();
    let repo = git2::Repository::open(&dir).unwrap();
    let hunk = |staged: bool| diff::workdir_file_diff(&repo, "f.txt", staged).unwrap().unwrap().hunks.remove(0);
    let h = hunk(false);
    let idx = |text: &str, kind: char| h.lines.iter().position(|l| l.text == text && l.kind == kind).unwrap();
    let index_content = || ops::git(&dir, &["show", ":f.txt"]).unwrap();

    // 只暂存 “b → B” 这一处修改
    let sel = [idx("b", '-'), idx("B", '+')];
    let p = diff::lines_patch(&repo, "f.txt", false, &h.header, Some(&sel), false).unwrap().unwrap();
    ops::apply_patch(&dir, &p, true, false).unwrap();
    assert_eq!(index_content(), "a\nB\nc\nd\n");

    // 再暂存新增的 x
    let h = hunk(false);
    let sel = [h.lines.iter().position(|l| l.text == "x" && l.kind == '+').unwrap()];
    let p = diff::lines_patch(&repo, "f.txt", false, &h.header, Some(&sel), false).unwrap().unwrap();
    ops::apply_patch(&dir, &p, true, false).unwrap();
    assert_eq!(index_content(), "a\nB\nc\nd\nx\n");

    // 从暂存区取消 “b → B”，x 还在暂存区
    let hs = hunk(true);
    let sel: Vec<usize> = hs.lines.iter().enumerate().filter(|(_, l)| l.text == "b" || l.text == "B").map(|(i, _)| i).collect();
    let p = diff::lines_patch(&repo, "f.txt", true, &hs.header, Some(&sel), true).unwrap().unwrap();
    ops::apply_patch(&dir, &p, true, true).unwrap();
    assert_eq!(index_content(), "a\nb\nc\nd\nx\n");

    // 丢弃工作区里新增的 y，其他改动保留
    let h = hunk(false);
    let sel = [h.lines.iter().position(|l| l.text == "y" && l.kind == '+').unwrap()];
    let p = diff::lines_patch(&repo, "f.txt", false, &h.header, Some(&sel), true).unwrap().unwrap();
    ops::apply_patch(&dir, &p, false, true).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "a\nB\nd\nx\n");

    // 只选了上下文行：报错
    let h = hunk(false);
    let ctx = h.lines.iter().position(|l| l.kind == ' ').unwrap();
    assert!(diff::lines_patch(&repo, "f.txt", false, &h.header, Some(&[ctx]), false).is_err());
}

#[test]
fn stage_selected_lines_without_trailing_newline() {
    use gittree_core::{diff, git2};
    let dir = temp_repo("lines-eol");
    std::fs::write(dir.join("f.txt"), "a\nlast").unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "初始", false).unwrap();
    std::fs::write(dir.join("f.txt"), "a\nnew\nLAST").unwrap();
    let repo = git2::Repository::open(&dir).unwrap();
    let h = diff::workdir_file_diff(&repo, "f.txt", false).unwrap().unwrap().hunks.remove(0);
    // 只暂存最后一行的修改，不要新增的 new
    let sel: Vec<usize> = h.lines.iter().enumerate().filter(|(_, l)| l.text == "last" || l.text == "LAST").map(|(i, _)| i).collect();
    let p = diff::lines_patch(&repo, "f.txt", false, &h.header, Some(&sel), false).unwrap().unwrap();
    ops::apply_patch(&dir, &p, true, false).unwrap();
    assert_eq!(ops::git(&dir, &["show", ":f.txt"]).unwrap(), "a\nLAST");
}

#[test]
fn compare_branches() {
    use gittree_core::git2;
    let dir = temp_repo("compare");
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "初始", false).unwrap();
    ops::git(&dir, &["switch", "-c", "release"]).unwrap();
    std::fs::write(dir.join("r.txt"), "r\n").unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "release 1", false).unwrap();
    std::fs::write(dir.join("a.txt"), "a\nrelease\n").unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "release 2", false).unwrap();
    ops::git(&dir, &["switch", "main"]).unwrap();
    std::fs::write(dir.join("m.txt"), "m\n").unwrap();
    ops::stage_all(&dir).unwrap();
    ops::commit(&dir, "main 1", false).unwrap();
    let repo = git2::Repository::open(&dir).unwrap();

    // 三点对比：只看 release 自己的改动，不包括 main 上新增的 m.txt
    let c = diff::compare(&repo, "main", "release", true).unwrap();
    let mut files: Vec<_> = c.files.iter().map(|f| (f.path.as_str(), f.status)).collect();
    files.sort();
    assert_eq!(files, [("a.txt", 'M'), ("r.txt", 'A')]);
    assert_eq!(c.commits.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(), ["release 2", "release 1"]);
    assert_eq!((c.ahead, c.behind), (2, 1));
    assert!(c.base.is_some());

    // 直接对比两端：main 上的 m.txt 在 release 里没有，算删除
    let c2 = diff::compare(&repo, "main", "release", false).unwrap();
    assert!(c2.files.iter().any(|f| f.path == "m.txt" && f.status == 'D'));

    let d = diff::compare_file_diff(&repo, c.base.as_ref().unwrap(), &c.to, "a.txt", None).unwrap().unwrap();
    assert!(d.hunks[0].lines.iter().any(|l| l.kind == '+' && l.text == "release"));

    assert!(diff::compare(&repo, "main", "no-such-branch", true).is_err());
}
