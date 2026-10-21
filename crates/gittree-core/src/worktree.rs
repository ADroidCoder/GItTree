//! 工作树（git worktree）：同一个仓库同时把不同的分支检出到不同的目录。

use crate::tr;
use crate::log::Error;
use crate::ops::git;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize, Default)]
pub struct Worktree {
    pub path: String,
    pub head: String,
    /// 检出的分支（短名称），分离头指针时为空
    pub branch: Option<String>,
    /// 主工作树（仓库本身所在的目录）
    pub main: bool,
    /// 就是当前打开的这个
    pub current: bool,
    pub locked: bool,
    /// 目录已经不存在了，可以清理
    pub prunable: bool,
}

pub fn list(dir: &Path) -> Result<Vec<Worktree>, Error> {
    let out = git(dir, &["worktree", "list", "--porcelain"])?;
    let here = git(dir, &["rev-parse", "--show-toplevel"]).unwrap_or_default();
    let mut list = parse(&out);
    if let Some(first) = list.first_mut() {
        first.main = true;
    }
    let here = normalize(here.trim());
    for w in &mut list {
        w.current = normalize(&w.path) == here;
    }
    Ok(list)
}

/// Windows 上 git 输出的路径大小写、斜杠可能不一样
fn normalize(p: &str) -> String {
    let p = p.trim_end_matches(['/', '\\']).replace('\\', "/");
    if cfg!(windows) { p.to_lowercase() } else { p }
}

fn parse(out: &str) -> Vec<Worktree> {
    let mut list = Vec::new();
    for block in out.split("\n\n").filter(|b| !b.trim().is_empty()) {
        let mut w = Worktree::default();
        for line in block.lines() {
            let (k, v) = line.split_once(' ').unwrap_or((line, ""));
            match k {
                "worktree" => w.path = v.to_string(),
                "HEAD" => w.head = v.to_string(),
                "branch" => w.branch = Some(v.strip_prefix("refs/heads/").unwrap_or(v).to_string()),
                "locked" => w.locked = true,
                "prunable" => w.prunable = true,
                _ => {}
            }
        }
        if !w.path.is_empty() {
            list.push(w);
        }
    }
    list
}

/// 新建工作树。new_branch 为 true 时从 start 新建分支 branch，否则检出已有的分支
pub fn add(dir: &Path, path: &str, branch: &str, new_branch: bool, start: Option<&str>) -> Result<String, Error> {
    let path = path.trim();
    if path.is_empty() {
        return Err(tr!("请填写工作树的目录", "Enter a folder for the worktree").into());
    }
    if new_branch {
        let mut args = vec!["worktree", "add", "-b", branch, path];
        args.extend(start);
        git(dir, &args)
    } else {
        git(dir, &["worktree", "add", path, branch])
    }
}

pub fn remove(dir: &Path, path: &str, force: bool) -> Result<String, Error> {
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(path);
    git(dir, &args)
}

/// 清理目录已经被删掉的工作树记录
pub fn prune(dir: &Path) -> Result<String, Error> {
    git(dir, &["worktree", "prune", "-v"])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain() {
        let out = "worktree /a/repo\nHEAD 111\nbranch refs/heads/main\n\nworktree /a/repo-dev\nHEAD 222\ndetached\nlocked\n\nworktree /gone\nHEAD 333\nbranch refs/heads/feature/x\nprunable gitdir file points to non-existent location\n";
        let w = parse(out);
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].branch.as_deref(), Some("main"));
        assert!(w[1].branch.is_none() && w[1].locked);
        assert_eq!(w[2].branch.as_deref(), Some("feature/x"));
        assert!(w[2].prunable);
    }
}
