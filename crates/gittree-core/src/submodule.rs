//! 子模块：列出、初始化、更新。

use crate::log::Error;
use crate::ops::git;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize, PartialEq)]
pub struct Submodule {
    pub name: String,
    /// 在仓库里的相对路径
    pub path: String,
    pub url: String,
    /// 父仓库记录的提交
    pub oid: String,
    /// ok：和记录的一致；uninitialized：还没有初始化（没有下载）；
    /// modified：子模块里检出的提交和父仓库记录的不一致；conflict：合并冲突
    pub state: String,
    /// git describe 的结果，例如 (v1.2.0)
    pub describe: String,
}

pub fn list(dir: &Path) -> Result<Vec<Submodule>, Error> {
    if !dir.join(".gitmodules").exists() {
        return Ok(Vec::new());
    }
    let out = git(dir, &["submodule", "status"])?;
    // .gitmodules 里的 名称 → (路径, 地址)
    let cfg = git(dir, &["config", "-f", ".gitmodules", "-z", "--get-regexp", r"^submodule\..*\.(path|url)$"]).unwrap_or_default();
    let mut names: Vec<(String, String, String)> = Vec::new();
    for rec in cfg.split('\0').filter(|r| !r.is_empty()) {
        let Some((key, value)) = rec.split_once('\n') else { continue };
        let Some(rest) = key.strip_prefix("submodule.") else { continue };
        let Some((name, field)) = rest.rsplit_once('.') else { continue };
        let i = match names.iter().position(|n| n.0 == name) {
            Some(i) => i,
            None => {
                names.push((name.to_string(), String::new(), String::new()));
                names.len() - 1
            }
        };
        if field == "path" {
            names[i].1 = value.to_string();
        } else {
            names[i].2 = value.to_string();
        }
    }
    Ok(out
        .lines()
        .filter_map(|l| {
            let mut sub = parse_status_line(l)?;
            if let Some((name, _, url)) = names.iter().find(|n| n.1 == sub.path) {
                sub.name = name.clone();
                sub.url = url.clone();
            }
            Some(sub)
        })
        .collect())
}

/// “-abc123 libs/foo”、“+abc123 libs/foo (v1.0-3-gabc)”、“ abc123 libs/foo (heads/main)”
fn parse_status_line(line: &str) -> Option<Submodule> {
    let flag = line.chars().next()?;
    let rest = &line[flag.len_utf8()..];
    let (oid, rest) = rest.split_once(' ')?;
    let (path, describe) = match rest.rsplit_once(" (") {
        Some((p, d)) if d.ends_with(')') => (p, format!("({d}")),
        _ => (rest, String::new()),
    };
    let state = match flag {
        '-' => "uninitialized",
        '+' => "modified",
        'U' => "conflict",
        _ => "ok",
    };
    Some(Submodule { name: path.to_string(), path: path.to_string(), url: String::new(), oid: oid.to_string(), state: state.into(), describe })
}

/// 初始化并更新到父仓库记录的提交。path 为空时更新全部（包括子模块里的子模块）
pub fn update(dir: &Path, path: Option<&str>) -> Result<String, Error> {
    let mut args = vec!["submodule", "update", "--init", "--recursive", "--progress"];
    if let Some(p) = path {
        args.extend(["--", p]);
    }
    git(dir, &args)
}

/// 把子模块更新到它远程分支上最新的提交（父仓库里会显示为改动，需要提交）
pub fn update_remote(dir: &Path, path: &str) -> Result<String, Error> {
    git(dir, &["submodule", "update", "--init", "--remote", "--progress", "--", path])
}

/// .gitmodules 里的地址改了以后，同步到本地配置
pub fn sync(dir: &Path, path: Option<&str>) -> Result<String, Error> {
    let mut args = vec!["submodule", "sync", "--recursive"];
    if let Some(p) = path {
        args.extend(["--", p]);
    }
    git(dir, &args)
}

pub fn add(dir: &Path, url: &str, path: &str) -> Result<String, Error> {
    git(dir, &["submodule", "add", "--progress", url.trim(), path.trim()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_submodule_status() {
        let s = parse_status_line("-1234abcd libs/my lib").unwrap();
        assert_eq!((s.path.as_str(), s.state.as_str(), s.describe.as_str()), ("libs/my lib", "uninitialized", ""));
        let s = parse_status_line("+1234abcd vendor/x (v1.0-3-gabc)").unwrap();
        assert_eq!((s.path.as_str(), s.state.as_str(), s.describe.as_str()), ("vendor/x", "modified", "(v1.0-3-gabc)"));
        let s = parse_status_line(" 1234abcd a (heads/main)").unwrap();
        assert_eq!(s.state, "ok");
    }
}
