//! 提交详情、文件列表和逐行 diff。

use crate::tr;
use crate::log::Error;
use git2::{Delta, Diff, DiffFindOptions, DiffOptions, Oid, Repository};
use serde::Serialize;

/// 单个文件超过这么多行就截断，防止界面一次渲染太多
const MAX_LINES: usize = 20_000;

#[derive(Debug, Clone, Serialize)]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    /// A 新增, M 修改, D 删除, R 重命名, C 复制, T 类型变化
    pub status: char,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
}

#[derive(Debug, Serialize)]
pub struct Person {
    pub name: String,
    pub email: String,
    pub time: i64,
    /// 时区偏移（分钟）
    pub offset: i32,
}

#[derive(Debug, Serialize)]
pub struct CommitDetails {
    pub oid: String,
    pub parents: Vec<String>,
    pub author: Person,
    pub committer: Person,
    pub message: String,
    pub files: Vec<FileChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffLine {
    /// '+' 新增, '-' 删除, ' ' 上下文
    pub kind: char,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
    pub truncated: bool,
}

fn person(sig: &git2::Signature) -> Person {
    Person {
        name: String::from_utf8_lossy(sig.name_bytes()).into_owned(),
        email: String::from_utf8_lossy(sig.email_bytes()).into_owned(),
        time: sig.when().seconds(),
        offset: sig.when().offset_minutes(),
    }
}

fn base_opts(paths: &[&str]) -> DiffOptions {
    let mut o = DiffOptions::new();
    o.context_lines(3).ignore_submodules(true);
    for p in paths {
        o.pathspec(*p);
    }
    if !paths.is_empty() {
        o.disable_pathspec_match(true);
    }
    o
}

fn find_renames(diff: &mut Diff) -> Result<(), git2::Error> {
    // 文件很多时重命名检测很慢，超过上限就跳过
    if diff.deltas().len() > 5000 {
        return Ok(());
    }
    let mut f = DiffFindOptions::new();
    f.renames(true).copies(false);
    diff.find_similar(Some(&mut f))
}

fn status_char(d: Delta) -> char {
    match d {
        Delta::Added | Delta::Untracked => 'A',
        Delta::Deleted => 'D',
        Delta::Renamed => 'R',
        Delta::Copied => 'C',
        Delta::Typechange => 'T',
        _ => 'M',
    }
}

fn path_str(p: Option<&std::path::Path>) -> String {
    p.map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default()
}

pub fn file_changes(diff: &Diff) -> Result<Vec<FileChange>, git2::Error> {
    let mut files: Vec<FileChange> = diff
        .deltas()
        .map(|d| {
            let new = path_str(d.new_file().path());
            let old = path_str(d.old_file().path());
            FileChange {
                old_path: (old != new && !old.is_empty()).then_some(old.clone()),
                path: if new.is_empty() { old } else { new },
                status: status_char(d.status()),
                additions: 0,
                deletions: 0,
                binary: d.flags().is_binary(),
            }
        })
        .collect();
    // 统计增删行数
    for (i, f) in files.iter_mut().enumerate() {
        if let Ok(Some(patch)) = git2::Patch::from_diff(diff, i) {
            if patch.delta().flags().is_binary() {
                f.binary = true;
            } else if let Ok((_, add, del)) = patch.line_stats() {
                f.additions = add as u32;
                f.deletions = del as u32;
            }
        }
    }
    Ok(files)
}

fn parent_tree<'r>(commit: &git2::Commit<'r>) -> Option<git2::Tree<'r>> {
    commit.parent(0).ok().and_then(|p| p.tree().ok())
}

pub fn commit_details(repo: &Repository, oid: &str) -> Result<CommitDetails, Error> {
    let commit = repo.find_commit(Oid::from_str(oid)?)?;
    let tree = commit.tree()?;
    let mut opts = base_opts(&[]);
    let mut diff = repo.diff_tree_to_tree(parent_tree(&commit).as_ref(), Some(&tree), Some(&mut opts))?;
    find_renames(&mut diff)?;
    Ok(CommitDetails {
        oid: commit.id().to_string(),
        parents: commit.parent_ids().map(|p| p.to_string()).collect(),
        author: person(&commit.author()),
        committer: person(&commit.committer()),
        message: String::from_utf8_lossy(commit.message_bytes()).into_owned(),
        files: file_changes(&diff)?,
    })
}

/// 把 diff 中第一个文件的内容转成界面用的结构
fn to_file_diff(diff: &Diff) -> Result<Option<FileDiff>, Error> {
    if diff.deltas().len() == 0 {
        return Ok(None);
    }
    let patch = git2::Patch::from_diff(diff, 0)?;
    let delta = diff.get_delta(0).unwrap();
    let new = path_str(delta.new_file().path());
    let old = path_str(delta.old_file().path());
    let mut out = FileDiff {
        old_path: (old != new && !old.is_empty()).then_some(old.clone()),
        path: if new.is_empty() { old } else { new },
        binary: delta.flags().is_binary(),
        hunks: Vec::new(),
        truncated: false,
    };
    let Some(patch) = patch else { return Ok(Some(out)) };
    if patch.delta().flags().is_binary() {
        out.binary = true;
        return Ok(Some(out));
    }
    let mut total = 0;
    for h in 0..patch.num_hunks() {
        let (hunk, n) = patch.hunk(h)?;
        let mut lines = Vec::with_capacity(n);
        for l in 0..n {
            let line = patch.line_in_hunk(h, l)?;
            let kind = match line.origin() {
                '+' | '>' => '+',
                '-' | '<' => '-',
                _ => ' ',
            };
            // “文件末尾没有换行” 的标记行不显示
            if matches!(line.origin(), '=' | '>' | '<') {
                continue;
            }
            let mut text = String::from_utf8_lossy(line.content()).into_owned();
            if text.ends_with('\n') {
                text.pop();
                if text.ends_with('\r') {
                    text.pop();
                }
            }
            lines.push(DiffLine { kind, old: line.old_lineno(), new: line.new_lineno(), text });
        }
        total += lines.len();
        out.hunks.push(Hunk {
            header: String::from_utf8_lossy(hunk.header()).trim_end().to_string(),
            lines,
        });
        if total > MAX_LINES {
            out.truncated = true;
            break;
        }
    }
    Ok(Some(out))
}

pub fn commit_file_diff(repo: &Repository, oid: &str, path: &str, old_path: Option<&str>) -> Result<Option<FileDiff>, Error> {
    let commit = repo.find_commit(Oid::from_str(oid)?)?;
    let tree = commit.tree()?;
    let mut paths = vec![path];
    paths.extend(old_path);
    let mut opts = base_opts(&paths);
    let mut diff = repo.diff_tree_to_tree(parent_tree(&commit).as_ref(), Some(&tree), Some(&mut opts))?;
    if old_path.is_some() {
        find_renames(&mut diff)?;
    }
    to_file_diff(&diff)
}

fn workdir_diff<'r>(repo: &'r Repository, path: &str, staged: bool) -> Result<Diff<'r>, Error> {
    let mut opts = base_opts(&[path]);
    if staged {
        let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        Ok(repo.diff_tree_to_index(head.as_ref(), None, Some(&mut opts))?)
    } else {
        opts.include_untracked(true).show_untracked_content(true).recurse_untracked_dirs(true);
        Ok(repo.diff_index_to_workdir(None, Some(&mut opts))?)
    }
}

/// 工作区文件的 diff。staged = true 看暂存区相对 HEAD 的改动，否则看工作区相对暂存区的改动。
pub fn workdir_file_diff(repo: &Repository, path: &str, staged: bool) -> Result<Option<FileDiff>, Error> {
    to_file_diff(&workdir_diff(repo, path, staged)?)
}

/// 生成只包含某一个代码块的补丁，用原始字节，避免非 UTF-8 文件（比如 GBK）被破坏。
/// 返回 None 表示代码块已经变了（界面上的 diff 过期了）。
pub fn hunk_patch(repo: &Repository, path: &str, staged: bool, header: &str) -> Result<Option<Vec<u8>>, Error> {
    let diff = workdir_diff(repo, path, staged)?;
    if diff.deltas().len() == 0 {
        return Ok(None);
    }
    let delta = diff.get_delta(0).unwrap();
    if !matches!(delta.status(), Delta::Modified) {
        return Err(tr!("只有修改过的文件才能按代码块暂存，新增或删除的文件请整个暂存", "Only modified files can be staged by hunk. Stage added or deleted files as a whole").into());
    }
    let Some(patch) = git2::Patch::from_diff(&diff, 0)? else { return Ok(None) };
    let file = delta.new_file().path().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    for h in 0..patch.num_hunks() {
        let (hunk, n) = patch.hunk(h)?;
        if String::from_utf8_lossy(hunk.header()).trim_end() != header {
            continue;
        }
        let mut out = Vec::new();
        out.extend_from_slice(format!("diff --git a/{file} b/{file}\n--- a/{file}\n+++ b/{file}\n").as_bytes());
        out.extend_from_slice(hunk.header());
        if !hunk.header().ends_with(b"\n") {
            out.push(b'\n');
        }
        for l in 0..n {
            let line = patch.line_in_hunk(h, l)?;
            match line.origin() {
                '+' | '-' | ' ' => {
                    out.push(line.origin() as u8);
                    out.extend_from_slice(line.content());
                    if !line.content().ends_with(b"\n") {
                        out.push(b'\n');
                    }
                }
                '=' | '>' | '<' => out.extend_from_slice(b"\\ No newline at end of file\n"),
                _ => {}
            }
        }
        return Ok(Some(out));
    }
    Ok(None)
}
