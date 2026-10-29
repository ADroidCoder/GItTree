//! 提交详情、文件列表和逐行 diff。

use crate::tr;
use crate::log::Error;
use git2::{Delta, Diff, DiffFindOptions, DiffOptions, Oid, Repository};
use serde::Serialize;

/// 单个文件超过这么多行就截断，防止界面一次渲染太多
const MAX_LINES: usize = 20_000;
/// 所有行的文字加起来超过这么多也截断：开头像文本、后面是二进制的文件可能有几十 MB 的一行
const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
/// 一行超过这么多字只显示前面的部分，太长的行界面排版会卡住
const MAX_LINE_CHARS: usize = 5000;
/// 提交的文件列表统计增删行数时，超过这么大的文件不逐行对比（libgit2 会把它当成二进制）
const MAX_STAT_SIZE: i64 = 16 * 1024 * 1024;
/// 预览文件差异时，超过这么大先不对比，让用户决定要不要看（可能只是不小心点到了）
const PREVIEW_SIZE: u64 = 2 * 1024 * 1024;

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
    /// 文件太大没有逐行对比：改动前后的大小（字节，没有这一边是 0）
    pub too_large: Option<(u64, u64)>,
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
    o.context_lines(3).ignore_submodules(true).max_size(MAX_STAT_SIZE);
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

/// 单个文件差异的选项。full：用户确认过要看大文件，不再按大小跳过（显示的行数和字数仍然有上限）
fn file_opts(paths: &[&str], full: bool) -> DiffOptions {
    let mut o = base_opts(paths);
    // 负数表示不按大小当成二进制
    o.max_size(if full { -1 } else { PREVIEW_SIZE as i64 });
    o
}

/// 把 diff 中第一个文件的内容转成界面用的结构
fn to_file_diff(diff: &Diff, full: bool) -> Result<Option<FileDiff>, Error> {
    if diff.deltas().len() == 0 {
        return Ok(None);
    }
    let delta = diff.get_delta(0).unwrap();
    let new = path_str(delta.new_file().path());
    let old = path_str(delta.old_file().path());
    let mut out = FileDiff {
        old_path: (old != new && !old.is_empty()).then_some(old.clone()),
        path: if new.is_empty() { old } else { new },
        binary: delta.flags().is_binary(),
        hunks: Vec::new(),
        truncated: false,
        too_large: None,
    };
    let sizes = (delta.old_file().size(), delta.new_file().size());
    if !full && sizes.0.max(sizes.1) > PREVIEW_SIZE {
        out.binary = true;
        out.too_large = Some(sizes);
        return Ok(Some(out));
    }
    let patch = git2::Patch::from_diff(diff, 0)?;
    let Some(patch) = patch else { return Ok(Some(out)) };
    if patch.delta().flags().is_binary() {
        out.binary = true;
        return Ok(Some(out));
    }
    let (mut total, mut bytes) = (0, 0);
    for h in 0..patch.num_hunks() {
        let (hunk, n) = patch.hunk(h)?;
        let mut lines = Vec::with_capacity(n.min(MAX_LINES));
        for l in 0..n {
            // 一个代码块可能就有几百万行（比如新增的大文件），要逐行检查
            if total + lines.len() >= MAX_LINES || bytes >= MAX_TEXT_BYTES {
                out.truncated = true;
                break;
            }
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
            // 一个字最多 4 个字节，先截一段再转换，不用把几十 MB 的一行整个转成字符串
            let content = line.content();
            let mut text = String::from_utf8_lossy(&content[..content.len().min(MAX_LINE_CHARS * 4 + 2)]).into_owned();
            if text.ends_with('\n') {
                text.pop();
                if text.ends_with('\r') {
                    text.pop();
                }
            }
            if let Some((cut, _)) = text.char_indices().nth(MAX_LINE_CHARS) {
                text.truncate(cut);
                text.push_str(" …");
            }
            bytes += text.len();
            lines.push(DiffLine { kind, old: line.old_lineno(), new: line.new_lineno(), text });
        }
        total += lines.len();
        out.hunks.push(Hunk {
            header: String::from_utf8_lossy(hunk.header()).trim_end().to_string(),
            lines,
        });
        if out.truncated {
            break;
        }
    }
    Ok(Some(out))
}

pub fn commit_file_diff(repo: &Repository, oid: &str, path: &str, old_path: Option<&str>, full: bool) -> Result<Option<FileDiff>, Error> {
    let commit = repo.find_commit(Oid::from_str(oid)?)?;
    let tree = commit.tree()?;
    let mut paths = vec![path];
    paths.extend(old_path);
    let mut opts = file_opts(&paths, full);
    let mut diff = repo.diff_tree_to_tree(parent_tree(&commit).as_ref(), Some(&tree), Some(&mut opts))?;
    if old_path.is_some() {
        find_renames(&mut diff)?;
    }
    to_file_diff(&diff, full)
}

fn workdir_diff<'r>(repo: &'r Repository, path: &str, staged: bool, full: bool) -> Result<Diff<'r>, Error> {
    let mut opts = file_opts(&[path], full);
    if staged {
        let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        Ok(repo.diff_tree_to_index(head.as_ref(), None, Some(&mut opts))?)
    } else {
        opts.include_untracked(true).show_untracked_content(true).recurse_untracked_dirs(true);
        Ok(repo.diff_index_to_workdir(None, Some(&mut opts))?)
    }
}

/// 工作区文件的 diff。staged = true 看暂存区相对 HEAD 的改动，否则看工作区相对暂存区的改动。
pub fn workdir_file_diff(repo: &Repository, path: &str, staged: bool, full: bool) -> Result<Option<FileDiff>, Error> {
    to_file_diff(&workdir_diff(repo, path, staged, full)?, full)
}

/// 生成只包含某一个代码块的补丁，用原始字节，避免非 UTF-8 文件（比如 GBK）被破坏。
/// 返回 None 表示代码块已经变了（界面上的 diff 过期了）。
pub fn hunk_patch(repo: &Repository, path: &str, staged: bool, header: &str) -> Result<Option<Vec<u8>>, Error> {
    lines_patch(repo, path, staged, header, None, false)
}

/// 和 hunk_patch 一样，但只包含代码块里选中的行（下标和界面上显示的行一致）。
/// reverse 表示补丁要反向应用（取消暂存、丢弃）：
/// 正向时没选中的新增行去掉、删除行变成上下文；反向时反过来。
pub fn lines_patch(
    repo: &Repository,
    path: &str,
    staged: bool,
    header: &str,
    selected: Option<&[usize]>,
    reverse: bool,
) -> Result<Option<Vec<u8>>, Error> {
    // 用户已经在看这个文件的差异了（大文件也是点过“仍然预览”的），按完整内容生成补丁
    let diff = workdir_diff(repo, path, staged, true)?;
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
        // 先把这个代码块的行收集起来；“文件末尾没有换行”的标记记在它前面那一行上
        struct Rec<'a> {
            origin: char,
            picked: bool,
            content: &'a [u8],
            no_eol: bool,
        }
        let mut recs: Vec<Rec> = Vec::new();
        // 界面上的行号：不算“文件末尾没有换行”的标记行
        let mut index = 0;
        for l in 0..n {
            let line = patch.line_in_hunk(h, l)?;
            match line.origin() {
                '=' | '>' | '<' => {
                    if let Some(last) = recs.last_mut() {
                        last.no_eol = true;
                    }
                }
                o @ ('+' | '-' | ' ') => {
                    recs.push(Rec { origin: o, picked: selected.is_none_or(|s| s.contains(&index)), content: line.content(), no_eol: false });
                    index += 1;
                }
                _ => {}
            }
        }

        let mut body = Vec::new();
        let (mut old_count, mut new_count, mut changed) = (0, 0, 0);
        let mut emit = |o: char, r: &Rec| {
            match o {
                ' ' => {
                    old_count += 1;
                    new_count += 1;
                }
                '-' => {
                    old_count += 1;
                    changed += 1;
                }
                _ => {
                    new_count += 1;
                    changed += 1;
                }
            }
            body.push(o as u8);
            body.extend_from_slice(r.content);
            if !r.content.ends_with(b"\n") {
                body.push(b'\n');
            }
            if r.no_eol {
                body.extend_from_slice(b"\\ No newline at end of file\n");
            }
        };
        // 没选中的行：在补丁应用的那一边本来就存在的变成上下文，不存在的去掉
        let keep = |r: &Rec| -> Option<char> {
            match r.origin {
                ' ' => Some(' '),
                o if r.picked => Some(o),
                '-' if !reverse => Some(' '),
                '+' if reverse => Some(' '),
                _ => None,
            }
        };
        // 连续的一组改动（先删除行后新增行）里，第 k 个删除行和第 k 个新增行交错输出，
        // 这样没选中的行变成上下文后，修改前后两边的行顺序都保持不变
        let mut i = 0;
        while i < recs.len() {
            if recs[i].origin == ' ' {
                emit(' ', &recs[i]);
                i += 1;
                continue;
            }
            let start = i;
            while i < recs.len() && recs[i].origin == '-' {
                i += 1;
            }
            let dels = &recs[start..i];
            let add_start = i;
            while i < recs.len() && recs[i].origin == '+' {
                i += 1;
            }
            let adds = &recs[add_start..i];
            for k in 0..dels.len().max(adds.len()) {
                if let Some(r) = dels.get(k)
                    && let Some(o) = keep(r)
                {
                    emit(o, r);
                }
                if let Some(r) = adds.get(k)
                    && let Some(o) = keep(r)
                {
                    emit(o, r);
                }
            }
        }
        if changed == 0 {
            return Err(tr!("没有选中要处理的改动行", "No changed lines are selected").into());
        }
        let mut out = Vec::new();
        out.extend_from_slice(format!("diff --git a/{file} b/{file}\n--- a/{file}\n+++ b/{file}\n").as_bytes());
        out.extend_from_slice(format!("@@ -{},{old_count} +{},{new_count} @@\n", hunk.old_start(), hunk.new_start()).as_bytes());
        out.extend_from_slice(&body);
        return Ok(Some(out));
    }
    Ok(None)
}

#[derive(Debug, Serialize)]
pub struct CompareCommit {
    pub oid: String,
    pub summary: String,
    pub author: String,
    pub time: i64,
}

#[derive(Debug, Serialize)]
pub struct Compare {
    /// 两端解析出来的提交
    pub from: String,
    pub to: String,
    /// 共同祖先（两边没有共同历史时为空）
    pub base: Option<String>,
    /// 文件改动：三点对比时是共同祖先 → to，否则是 from → to
    pub files: Vec<FileChange>,
    /// to 有、from 没有的提交，新的在前
    pub commits: Vec<CompareCommit>,
    /// commits 只列出前 MAX 个，这是实际总数
    pub ahead: usize,
    /// from 有、to 没有的提交数
    pub behind: usize,
}

const COMPARE_MAX_COMMITS: usize = 500;

fn resolve<'r>(repo: &'r Repository, rev: &str) -> Result<git2::Commit<'r>, Error> {
    let obj = repo
        .revparse_single(rev.trim())
        .map_err(|_| tr!("找不到 {}，请输入分支名、标签名或提交号", "Can't find {}. Enter a branch, tag or commit hash", rev.trim()))?;
    Ok(obj.peel_to_commit()?)
}

/// 对比两个版本。three_dot 为 true 时只看 to 相对共同祖先新增的改动（和 GitHub 的 Compare 一样），
/// 否则直接对比两端
pub fn compare(repo: &Repository, from: &str, to: &str, three_dot: bool) -> Result<Compare, Error> {
    let a = resolve(repo, from)?;
    let b = resolve(repo, to)?;
    let base = repo.merge_base(a.id(), b.id()).ok();
    let old_tree = match (three_dot, base) {
        (true, Some(base)) => repo.find_commit(base)?.tree()?,
        _ => a.tree()?,
    };
    let mut opts = base_opts(&[]);
    let mut diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&b.tree()?), Some(&mut opts))?;
    find_renames(&mut diff)?;

    let (ahead, behind) = repo.graph_ahead_behind(b.id(), a.id())?;
    let mut walk = repo.revwalk()?;
    walk.push(b.id())?;
    walk.hide(a.id())?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;
    let mut commits = Vec::new();
    for oid in walk.take(COMPARE_MAX_COMMITS) {
        let c = repo.find_commit(oid?)?;
        commits.push(CompareCommit {
            oid: c.id().to_string(),
            summary: String::from_utf8_lossy(c.summary_bytes().unwrap_or_default()).into_owned(),
            author: String::from_utf8_lossy(c.author().name_bytes()).into_owned(),
            time: c.time().seconds(),
        });
    }
    Ok(Compare {
        from: a.id().to_string(),
        to: b.id().to_string(),
        base: base.map(|o| o.to_string()),
        files: file_changes(&diff)?,
        commits,
        ahead,
        behind,
    })
}

/// 两个提交之间某个文件的 diff（from 和 to 都是提交号）
pub fn compare_file_diff(repo: &Repository, from: &str, to: &str, path: &str, old_path: Option<&str>, full: bool) -> Result<Option<FileDiff>, Error> {
    let a = repo.find_commit(Oid::from_str(from)?)?.tree()?;
    let b = repo.find_commit(Oid::from_str(to)?)?.tree()?;
    let mut paths = vec![path];
    paths.extend(old_path);
    let mut opts = file_opts(&paths, full);
    let mut diff = repo.diff_tree_to_tree(Some(&a), Some(&b), Some(&mut opts))?;
    if old_path.is_some() {
        find_renames(&mut diff)?;
    }
    to_file_diff(&diff, full)
}

#[derive(Debug, Serialize)]
pub struct ImageSide {
    /// base64 编码的文件内容；文件太大或是没下载的 LFS 文件时为空
    pub data: Option<String>,
    pub size: u64,
    /// 是 Git LFS 管理的文件
    pub lfs: bool,
}

#[derive(Debug, Serialize)]
pub struct ImagePair {
    pub old: Option<ImageSide>,
    pub new: Option<ImageSide>,
}

/// 图片超过这么大就不显示
const MAX_IMAGE: usize = 30 * 1024 * 1024;

/// 图片差异的两边：改动前和改动后的文件内容。
/// source 和界面上的差异一样：commit（oid）、compare（from → to）、workdir（staged 表示暂存区 → HEAD）
#[allow(clippy::too_many_arguments)]
pub fn image_pair(
    repo: &Repository,
    source: &str,
    oid: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    staged: bool,
    path: &str,
    old_path: Option<&str>,
) -> Result<ImagePair, Error> {
    let old_path = old_path.unwrap_or(path);
    let at_commit = |c: Option<git2::Commit>, p: &str| -> Option<Vec<u8>> {
        let tree = c?.tree().ok()?;
        let entry = tree.get_path(std::path::Path::new(p)).ok()?;
        Some(repo.find_blob(entry.id()).ok()?.content().to_vec())
    };
    let commit = |rev: Option<&str>| rev.and_then(|r| repo.revparse_single(r).ok()).and_then(|o| o.peel_to_commit().ok());
    let in_index = |p: &str| -> Option<Vec<u8>> {
        let mut index = repo.index().ok()?;
        // 暂存区可能在这个 Repository 打开后被别的 git 命令改过
        index.read(false).ok()?;
        let entry = index.get_path(std::path::Path::new(p), 0)?;
        Some(repo.find_blob(entry.id).ok()?.content().to_vec())
    };
    let (old, new) = match source {
        "commit" => {
            let c = commit(oid);
            (at_commit(c.as_ref().and_then(|c| c.parent(0).ok()), old_path), at_commit(c, path))
        }
        "compare" => (at_commit(commit(from), old_path), at_commit(commit(to), path)),
        _ if staged => (at_commit(commit(Some("HEAD")), old_path), in_index(path)),
        _ => {
            let workdir = repo.workdir().ok_or_else(|| tr!("暂不支持裸仓库", "Bare repositories aren't supported yet"))?;
            (in_index(path), std::fs::read(workdir.join(path)).ok())
        }
    };
    let dir = repo.workdir().map(|d| d.to_path_buf());
    let side = |data: Option<Vec<u8>>| {
        data.map(|mut d| {
            let mut lfs = false;
            if crate::lfs::is_pointer(&d) {
                lfs = true;
                let size = crate::lfs::pointer_size(&d).unwrap_or(0);
                match dir.as_deref().and_then(|dir| crate::ops::without_prompt(|| crate::lfs::smudge(dir, &d).ok())) {
                    Some(real) if !crate::lfs::is_pointer(&real) => d = real,
                    _ => return ImageSide { data: None, size, lfs },
                }
            }
            let size = d.len() as u64;
            ImageSide { data: (d.len() <= MAX_IMAGE).then(|| base64(&d)), size, lfs }
        })
    };
    Ok(ImagePair { old: side(old), new: side(new) })
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_reference() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(super::base64(&[0xff, 0xfe, 0x00, 0x10]), "//4AEA==");
    }
}
