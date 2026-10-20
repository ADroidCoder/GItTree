//! 文件历史和逐行追溯（blame），都交给 git 命令：它能跟踪重命名，结果和命令行一致。

use crate::log::Error;
use crate::ops::git;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct HistoryEntry {
    pub oid: String,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub summary: String,
    /// 这个提交里文件的路径（重命名前的提交路径不一样）
    pub path: String,
    pub old_path: Option<String>,
    /// A M D R …
    pub status: String,
}

/// 修改过这个文件的提交，最新的在前，自动跟踪重命名
pub fn file_history(dir: &Path, path: &str, limit: usize) -> Result<Vec<HistoryEntry>, Error> {
    let n = limit.to_string();
    let out = git(
        dir,
        &[
            "-c",
            "core.quotepath=false",
            "log",
            "--follow",
            "-M",
            "--name-status",
            "--format=%x1e%H%x1f%an%x1f%ae%x1f%at%x1f%s",
            "-n",
            &n,
            "--",
            path,
        ],
    )?;
    let mut list = Vec::new();
    let mut cur_path = path.to_string();
    for rec in out.split('\x1e').filter(|r| !r.trim().is_empty()) {
        let mut lines = rec.lines();
        let head: Vec<&str> = lines.next().unwrap_or("").split('\x1f').collect();
        if head.len() < 5 {
            continue;
        }
        let (mut status, mut old_path) = ("M".to_string(), None);
        if let Some(ns) = lines.find(|l| !l.trim().is_empty()) {
            let parts: Vec<&str> = ns.split('\t').collect();
            status = parts[0].chars().next().map(String::from).unwrap_or_default();
            match parts.len() {
                3 => {
                    old_path = Some(parts[1].to_string());
                    cur_path = parts[2].to_string();
                }
                2 => cur_path = parts[1].to_string(),
                _ => {}
            }
        }
        list.push(HistoryEntry {
            oid: head[0].to_string(),
            author: head[1].to_string(),
            email: head[2].to_string(),
            time: head[3].parse().unwrap_or(0),
            summary: head[4..].join("\x1f"),
            path: cur_path.clone(),
            old_path: old_path.clone(),
            status,
        });
        // 更早的提交里文件叫旧名字
        if let Some(o) = old_path {
            cur_path = o;
        }
    }
    Ok(list)
}

#[derive(Debug, Clone, Serialize)]
pub struct BlameCommit {
    pub oid: String,
    pub author: String,
    pub time: i64,
    pub summary: String,
    /// 还没有提交的改动
    pub uncommitted: bool,
}

#[derive(Debug, Serialize)]
pub struct Blame {
    pub commits: Vec<BlameCommit>,
    /// 每一行的内容
    pub lines: Vec<String>,
    /// 每一行来自 commits 里的哪一个
    pub line_commit: Vec<u32>,
}

/// 逐行追溯；rev 为空时追溯工作区里的文件（包括没提交的改动）
pub fn blame(dir: &Path, path: &str, rev: Option<&str>) -> Result<Blame, Error> {
    let mut args = vec!["blame", "--porcelain"];
    if let Some(r) = rev {
        args.push(r);
    }
    args.extend(["--", path]);
    let out = git(dir, &args)?;

    let mut commits: Vec<BlameCommit> = Vec::new();
    let mut index: HashMap<String, u32> = HashMap::new();
    let mut lines = Vec::new();
    let mut line_commit = Vec::new();
    let mut cur: u32 = 0;
    let mut expect_header = true;
    for l in out.split('\n') {
        if let Some(content) = l.strip_prefix('\t') {
            lines.push(content.strip_suffix('\r').unwrap_or(content).to_string());
            line_commit.push(cur);
            expect_header = true;
            continue;
        }
        if expect_header {
            let Some(oid) = l.split(' ').next().filter(|s| s.len() >= 40) else { continue };
            cur = *index.entry(oid.to_string()).or_insert_with(|| {
                commits.push(BlameCommit {
                    oid: oid.to_string(),
                    author: String::new(),
                    time: 0,
                    summary: String::new(),
                    uncommitted: oid.bytes().all(|b| b == b'0'),
                });
                (commits.len() - 1) as u32
            });
            expect_header = false;
            continue;
        }
        let c = &mut commits[cur as usize];
        if let Some(v) = l.strip_prefix("author ") {
            c.author = v.to_string();
        } else if let Some(v) = l.strip_prefix("author-time ") {
            c.time = v.parse().unwrap_or(0);
        } else if let Some(v) = l.strip_prefix("summary ") {
            c.summary = v.to_string();
        }
    }
    Ok(Blame { commits, lines, line_commit })
}
