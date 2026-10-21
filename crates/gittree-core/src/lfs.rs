//! Git LFS：大文件存在单独的服务器上，仓库里只保存一个很小的指针文件。

use crate::log::Error;
use crate::ops::{git, git_bytes};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct LfsInfo {
    /// 装了 git-lfs
    pub installed: bool,
    /// 仓库用了 LFS（.gitattributes 里有 filter=lfs）
    pub used: bool,
    /// .gitattributes 里交给 LFS 管理的文件规则，例如 *.psd
    pub patterns: Vec<String>,
}

pub fn info(dir: &Path) -> LfsInfo {
    let patterns: Vec<String> = std::fs::read_to_string(dir.join(".gitattributes"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && l.split_whitespace().any(|a| a == "filter=lfs"))
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    LfsInfo { installed: git(dir, &["lfs", "version"]).is_ok(), used: !patterns.is_empty(), patterns }
}

pub fn pull(dir: &Path) -> Result<String, Error> {
    git(dir, &["lfs", "pull"])
}

pub fn fetch(dir: &Path) -> Result<String, Error> {
    git(dir, &["lfs", "fetch"])
}

/// 让 LFS 管理一类文件，例如 *.psd（会修改 .gitattributes，需要提交）
pub fn track(dir: &Path, pattern: &str) -> Result<String, Error> {
    git(dir, &["lfs", "track", pattern.trim()])
}

pub fn untrack(dir: &Path, pattern: &str) -> Result<String, Error> {
    git(dir, &["lfs", "untrack", pattern.trim()])
}

const POINTER: &[u8] = b"version https://git-lfs.github.com/spec/v1";

/// 内容是不是 LFS 指针文件
pub fn is_pointer(data: &[u8]) -> bool {
    data.len() < 1024 && data.starts_with(POINTER)
}

/// 指针文件里记录的真实文件大小
pub fn pointer_size(data: &[u8]) -> Option<u64> {
    let text = std::str::from_utf8(data).ok()?;
    text.lines().find_map(|l| l.strip_prefix("size ")).and_then(|s| s.trim().parse().ok())
}

/// 用指针换回真实内容（本地缓存里没有时 git-lfs 会去服务器下载）
pub fn smudge(dir: &Path, pointer: &[u8]) -> Result<Vec<u8>, Error> {
    git_bytes(dir, &["lfs", "smudge"], Some(pointer))
}
