//! 导出 / 应用补丁，浅克隆和部分检出

use gittree_core::{ops, patch, sparse};
use std::path::{Path, PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gittree-patch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn setup(dir: &Path) {
    ops::git(dir, &["config", "user.name", "测试者"]).unwrap();
    ops::git(dir, &["config", "user.email", "t@example.com"]).unwrap();
    ops::git(dir, &["config", "commit.gpgsign", "false"]).unwrap();
    ops::git(dir, &["config", "core.autocrlf", "false"]).unwrap();
}

fn temp_repo(name: &str) -> PathBuf {
    let dir = temp_dir(name);
    ops::git(&dir, &["init", "-q", "-b", "main"]).unwrap();
    setup(&dir);
    dir
}

fn commit_file(dir: &Path, file: &str, content: &str, msg: &str) -> String {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
    ops::stage_all(dir).unwrap();
    ops::commit(dir, msg, false).unwrap();
    ops::git(dir, &["rev-parse", "HEAD"]).unwrap().trim().to_string()
}

fn status(dir: &Path) -> ops::Status {
    let git_dir = PathBuf::from(ops::git(dir, &["rev-parse", "--absolute-git-dir"]).unwrap().trim());
    ops::status(dir, &git_dir).unwrap()
}

/// 补丁导出后在另一个仓库里应用成同样说明、同样作者的提交
#[test]
fn export_and_apply_patches() {
    let src = temp_repo("src");
    commit_file(&src, "a.txt", "1\n", "第一个");
    let dst = temp_dir("dst");
    ops::git(&dst, &["clone", "-q", src.to_str().unwrap(), "."]).unwrap();
    setup(&dst);
    let b = commit_file(&src, "a.txt", "1\n2\n", "加一行");
    let c = commit_file(&src, "b.txt", "新文件\n", "新增 b");

    // 复制成文本：两个提交按顺序拼在一起
    let text = patch::export_text(&src, &[b.clone(), c.clone()]).unwrap();
    assert!(patch::is_mail(&text));
    // 中文说明在邮件头里会编码，按补丁头的个数数
    assert_eq!(text.matches("\nSubject: ").count() + text.starts_with("Subject: ") as usize, 2);

    // 导出成文件，编号从 0001 开始
    let out = temp_dir("out");
    let files = patch::export_files(&src, &[b, c], &out).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files[0].contains("0001-") && files[1].contains("0002-"));

    patch::apply(&dst, &files, None).unwrap();
    let log = ops::git(&dst, &["log", "--format=%s|%an", "-3"]).unwrap();
    assert_eq!(log.lines().collect::<Vec<_>>(), ["新增 b|测试者", "加一行|测试者", "第一个|测试者"]);
    assert_eq!(std::fs::read_to_string(dst.join("b.txt")).unwrap(), "新文件\n");

    // 粘贴的文本也一样能用
    let again = temp_dir("again");
    ops::git(&again, &["clone", "-q", src.to_str().unwrap(), "."]).unwrap();
    setup(&again);
    ops::git(&again, &["reset", "-q", "--hard", "HEAD~2"]).unwrap();
    patch::apply(&again, &[], Some(&text)).unwrap();
    assert_eq!(ops::git(&again, &["log", "--format=%s", "-1"]).unwrap().trim(), "新增 b");
}

/// git diff 输出的普通差异放到工作区，不生成提交
#[test]
fn apply_plain_diff_to_working_tree() {
    let dir = temp_repo("plain");
    commit_file(&dir, "a.txt", "1\n", "第一个");
    std::fs::write(dir.join("a.txt"), "1\n改了\n").unwrap();
    let diff = ops::git(&dir, &["diff"]).unwrap();
    ops::git(&dir, &["checkout", "--", "a.txt"]).unwrap();
    assert!(!patch::is_mail(&diff));

    patch::apply(&dir, &[], Some(&diff)).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "1\n改了\n");
    assert_eq!(ops::git(&dir, &["rev-list", "--count", "HEAD"]).unwrap().trim(), "1");
    assert!(patch::apply(&dir, &[], Some("  \n")).is_err());
}

/// 合并提交没有补丁
#[test]
fn merge_commit_has_no_patch() {
    let dir = temp_repo("merge");
    commit_file(&dir, "a.txt", "1\n", "第一个");
    ops::git(&dir, &["checkout", "-q", "-b", "side"]).unwrap();
    commit_file(&dir, "b.txt", "b\n", "旁支");
    ops::git(&dir, &["checkout", "-q", "main"]).unwrap();
    commit_file(&dir, "c.txt", "c\n", "主线");
    ops::git(&dir, &["merge", "-q", "--no-edit", "side"]).unwrap();
    let merge = ops::git(&dir, &["rev-parse", "HEAD"]).unwrap().trim().to_string();
    assert!(patch::export_text(&dir, &[merge.clone()]).is_err());
    // 和普通提交一起导出时只跳过合并提交
    let side = ops::git(&dir, &["rev-parse", "side"]).unwrap().trim().to_string();
    let out = temp_dir("merge-out");
    assert_eq!(patch::export_files(&dir, &[merge, side], &out).unwrap().len(), 1);
}

/// 补丁对不上时停下，状态显示为 am，可以中止
#[test]
fn conflicting_patch_stops_as_am() {
    let src = temp_repo("am-src");
    commit_file(&src, "a.txt", "1\n", "第一个");
    let dst = temp_dir("am-dst");
    ops::git(&dst, &["clone", "-q", src.to_str().unwrap(), "."]).unwrap();
    setup(&dst);
    let b = commit_file(&src, "a.txt", "这边\n", "改 a");
    commit_file(&dst, "a.txt", "那边\n", "也改 a");
    let text = patch::export_text(&src, &[b]).unwrap();

    assert!(patch::apply(&dst, &[], Some(&text)).is_err());
    let st = status(&dst);
    assert_eq!(st.in_progress.as_deref(), Some("am"));
    assert_eq!(st.conflicted.len(), 1);
    ops::sequencer(&dst, "am", "abort").unwrap();
    assert_eq!(status(&dst).in_progress, None);
    assert_eq!(ops::git(&dst, &["log", "--format=%s", "-1"]).unwrap().trim(), "也改 a");
}

/// 浅克隆只有最近的提交，可以往前补、补全
#[test]
fn shallow_clone_and_deepen() {
    let src = temp_repo("shallow-src");
    for i in 0..5 {
        commit_file(&src, "a.txt", &format!("{i}\n"), &format!("提交 {i}"));
    }
    ops::git(&src, &["branch", "other", "HEAD~1"]).unwrap();
    let parent = temp_dir("shallow");
    let url = format!("file://{}", src.display());
    let opts = ops::CloneOptions { depth: Some(2), sparse: None };
    let dir = ops::clone(&parent, &url, "副本", &opts).unwrap();
    let count = || ops::git(&dir, &["rev-list", "--count", "--all"]).unwrap().trim().parse::<u32>().unwrap();

    let info = sparse::info(&dir);
    assert!(info.shallow && !info.lazy && info.sparse.is_none());
    // 其他分支也克隆了
    assert!(ops::git(&dir, &["branch", "-r"]).unwrap().contains("origin/other"));
    assert!(count() < 5);
    // 浅克隆的历史能正常读出来，最早的提交没有父提交
    let log = gittree_core::log::Log::load(&dir).unwrap();
    assert_eq!(log.oids.len() as u32, count());

    sparse::deepen(&dir, Some(1)).unwrap();
    assert!(sparse::info(&dir).shallow);
    sparse::deepen(&dir, None).unwrap();
    assert!(!sparse::info(&dir).shallow);
    assert_eq!(count(), 5);
}

/// 部分检出：只有选中的目录，可以改选、关掉
#[test]
fn sparse_clone_and_change_dirs() {
    let src = temp_repo("sparse-src");
    commit_file(&src, "根.txt", "r\n", "根目录");
    commit_file(&src, "app/main.rs", "fn main() {}\n", "app");
    commit_file(&src, "app/sub/x.rs", "x\n", "sub");
    commit_file(&src, "docs/readme.md", "# 文档\n", "docs");
    ops::git(&src, &["config", "uploadpack.allowFilter", "true"]).unwrap();
    let parent = temp_dir("sparse");
    let url = format!("file://{}", src.display());
    let opts = ops::CloneOptions { depth: None, sparse: Some(vec!["app".into()]) };
    let dir = ops::clone(&parent, &url, "副本", &opts).unwrap();

    let info = sparse::info(&dir);
    assert!(info.lazy);
    assert_eq!(info.sparse, Some(vec!["app".to_string()]));
    assert!(dir.join("根.txt").exists() && dir.join("app/sub/x.rs").exists());
    assert!(!dir.join("docs").exists());

    assert_eq!(sparse::subdirs(&dir, "").unwrap(), ["app", "docs"]);
    assert_eq!(sparse::subdirs(&dir, "app").unwrap(), ["app/sub"]);

    // 没检出的文件，看提交差异前会先下载
    let docs = ops::git(&dir, &["log", "--format=%H", "-1", "--", "docs"]).unwrap().trim().to_string();
    sparse::prefetch(&dir, &docs, None).unwrap();

    sparse::set(&dir, &["docs/".into()]).unwrap();
    assert_eq!(sparse::info(&dir).sparse, Some(vec!["docs".to_string()]));
    assert!(dir.join("docs/readme.md").exists() && !dir.join("app").exists());
    // 工作区没有多出改动
    assert!(status(&dir).unstaged.is_empty());

    sparse::disable(&dir).unwrap();
    assert!(sparse::info(&dir).sparse.is_none());
    assert!(dir.join("app/main.rs").exists());
}
