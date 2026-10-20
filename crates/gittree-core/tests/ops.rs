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
