//! 只下载一部分的仓库：浅克隆（只有最近的历史）、部分检出（sparse-checkout，只检出一些目录）。
//!
//! 部分检出的仓库克隆时加了 --filter=blob:none：提交和目录结构都下载，文件内容用到时才下载。

use crate::log::Error;
use crate::ops::git;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Default, Serialize)]
pub struct PartialInfo {
    /// 浅克隆：更早的历史没有下载
    pub shallow: bool,
    /// 文件内容按需下载（--filter 克隆）
    pub lazy: bool,
    /// 开了部分检出时，检出的目录（空表示只有根目录下的文件）
    pub sparse: Option<Vec<String>>,
}

pub fn info(dir: &Path) -> PartialInfo {
    let sparse = git(dir, &["config", "--bool", "core.sparseCheckout"]).is_ok_and(|v| v.trim() == "true");
    PartialInfo {
        shallow: git(dir, &["rev-parse", "--is-shallow-repository"]).is_ok_and(|v| v.trim() == "true"),
        lazy: is_lazy(dir),
        sparse: sparse.then(|| {
            git(dir, &["sparse-checkout", "list"])
                .unwrap_or_default()
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        }),
    }
}

/// 文件内容是不是用到时才从服务器下载
pub fn is_lazy(dir: &Path) -> bool {
    git(dir, &["config", "--get-regexp", r"^remote\..*\.promisor$"]).is_ok_and(|out| out.lines().any(|l| l.trim_end().ends_with(" true")))
}

/// 浅克隆往前多下载 more 个提交；None 表示下载全部历史
pub fn deepen(dir: &Path, more: Option<u32>) -> Result<String, Error> {
    let arg = match more {
        Some(n) => format!("--deepen={n}"),
        None => "--unshallow".into(),
    };
    git(dir, &["fetch", "--progress", &arg])
}

/// 只检出这些目录（以及根目录下的文件）；还没开部分检出时会打开
pub fn set(dir: &Path, dirs: &[String]) -> Result<String, Error> {
    let mut args = vec!["sparse-checkout", "set", "--cone"];
    args.extend(dirs.iter().map(|d| d.trim().trim_matches('/')).filter(|d| !d.is_empty()));
    git(dir, &args)
}

/// 关掉部分检出，检出全部文件
pub fn disable(dir: &Path) -> Result<String, Error> {
    git(dir, &["sparse-checkout", "disable"])
}

/// HEAD 里 parent 目录下的子目录（parent 为空时是根目录），用来选择要检出的目录
pub fn subdirs(dir: &Path, parent: &str) -> Result<Vec<String>, Error> {
    let parent = parent.trim_matches('/');
    let prefix = if parent.is_empty() { String::new() } else { format!("{parent}/") };
    let mut args = vec!["ls-tree", "-d", "--name-only", "-z", "HEAD"];
    if !prefix.is_empty() {
        args.extend(["--", prefix.as_str()]);
    }
    let out = git(dir, &args)?;
    Ok(out.split('\0').filter(|s| !s.is_empty()).map(str::to_string).collect())
}

/// 文件内容按需下载的仓库里，先用 git 把差异要用的文件内容下载下来（读差异用的 libgit2 不会自己去服务器下载）。
/// range 为空时是 commit 这一个提交的改动，否则是 commit 和 range 之间的对比
pub fn prefetch(dir: &Path, commit: &str, range: Option<(&str, bool)>) -> Result<(), Error> {
    match range {
        None => git(dir, &["diff-tree", "-r", "--root", "--stat", commit])?,
        Some((to, three_dot)) => {
            let spec = format!("{commit}{}{to}", if three_dot { "..." } else { ".." });
            git(dir, &["diff", "--stat", &spec])?
        }
    };
    Ok(())
}
