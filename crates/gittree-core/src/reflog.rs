//! Reflog：HEAD 和分支移动过的记录，用来找回 reset 过头、误删分支后丢失的提交。

use crate::log::Error;
use crate::ops::git;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct ReflogEntry {
    /// 这一步之后 HEAD（或分支）指向的提交
    pub oid: String,
    /// 发生的时间（秒）
    pub time: i64,
    /// 动作：commit、checkout、reset、rebase、merge、pull、cherry-pick 等
    pub action: String,
    /// git 记录的说明，例如 “moving from main to dev”
    pub message: String,
    /// 提交本身的说明
    pub summary: String,
    pub author: String,
}

/// 最近的 reflog 记录，新的在前。target 为空时看 HEAD
pub fn reflog(dir: &Path, target: Option<&str>, limit: usize) -> Result<Vec<ReflogEntry>, Error> {
    let target = target.filter(|t| !t.is_empty()).unwrap_or("HEAD");
    let n = format!("-n{limit}");
    // --date=unix 时 %gd 是 HEAD@{1700000000}，从里面取时间
    let out = match git(dir, &["log", "-g", &n, "--date=unix", "--format=%H%x00%gd%x00%gs%x00%s%x00%an", target, "--"]) {
        Ok(out) => out,
        // 新仓库还没有任何记录
        Err(_) if git(dir, &["rev-parse", "-q", "--verify", target]).is_err() => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    Ok(out.lines().filter_map(parse_line).collect())
}

fn parse_line(line: &str) -> Option<ReflogEntry> {
    let mut p = line.split('\0');
    let oid = p.next()?.to_string();
    let selector = p.next()?;
    let subject = p.next()?;
    let summary = p.next().unwrap_or("").to_string();
    let author = p.next().unwrap_or("").to_string();
    let time = selector
        .rsplit_once("@{")
        .and_then(|(_, t)| t.strip_suffix('}'))
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);
    // “checkout: moving from a to b”、“commit (amend): xxx”、“reset: moving to HEAD~1”
    let (action, message) = match subject.split_once(": ") {
        Some((a, m)) => (a.to_string(), m.to_string()),
        None => (subject.to_string(), String::new()),
    };
    Some(ReflogEntry { oid, time, action, message, summary, author })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reflog_lines() {
        let e = parse_line("abc\0HEAD@{1700000000}\0reset: moving to HEAD~2\0修复\0张三").unwrap();
        assert_eq!(e.time, 1_700_000_000);
        assert_eq!(e.action, "reset");
        assert_eq!(e.message, "moving to HEAD~2");
        assert_eq!(e.summary, "修复");
        let e = parse_line("abc\0HEAD@{1}\0commit (initial): first\0first\0a").unwrap();
        assert_eq!(e.action, "commit (initial)");
    }
}
