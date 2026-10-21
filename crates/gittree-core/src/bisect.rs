//! 二分查错（git bisect）：标记好的和坏的提交，git 每次检出中间的一个，最后找到第一个有问题的提交。

use crate::tr;
use crate::log::Error;
use crate::ops::git;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize, Default)]
pub struct BisectState {
    pub active: bool,
    pub bad: Option<String>,
    pub good: Vec<String>,
    pub skipped: Vec<String>,
    /// 现在检出的、等待标记的提交
    pub current: Option<String>,
    /// 还剩多少个提交可能有问题
    pub remaining: Option<u32>,
    /// 已经找到的第一个有问题的提交
    pub found: Option<String>,
}

/// 只读 .git 里的文件和引用，开销很小，每次刷新都可以调用
pub fn state(dir: &Path, git_dir: &Path) -> Result<BisectState, Error> {
    let Ok(log) = std::fs::read_to_string(git_dir.join("BISECT_LOG")) else {
        return Ok(BisectState::default());
    };
    let mut st = BisectState { active: true, ..Default::default() };
    for line in log.lines() {
        // # first bad commit: [abc...] 说明（新版 git 写成 # first 'bad' commit: [...]）
        if let Some(rest) = line.strip_prefix("# first ").and_then(|r| r.split_once("commit: [")).map(|(_, r)| r) {
            st.found = rest.split(']').next().map(str::to_string);
        }
    }
    let refs = git(dir, &["for-each-ref", "--format=%(refname) %(objectname)", "refs/bisect/"]).unwrap_or_default();
    for line in refs.lines() {
        let Some((name, oid)) = line.split_once(' ') else { continue };
        let name = name.trim_start_matches("refs/bisect/");
        if name == "bad" {
            st.bad = Some(oid.to_string());
        } else if name.starts_with("good") {
            st.good.push(oid.to_string());
        } else if name.starts_with("skip") {
            st.skipped.push(oid.to_string());
        }
    }
    st.current = git(dir, &["rev-parse", "-q", "--verify", "HEAD"]).ok().map(|s| s.trim().to_string());
    if st.found.is_none()
        && let Some(bad) = &st.bad
        && !st.good.is_empty()
    {
        let mut args = vec!["rev-list", "--count", bad.as_str()];
        let nots: Vec<String> = st.good.iter().map(|g| format!("^{g}")).collect();
        args.extend(nots.iter().map(String::as_str));
        st.remaining = git(dir, &args).ok().and_then(|n| n.trim().parse().ok());
    }
    Ok(st)
}

/// 开始二分查错：bad 是有问题的提交，good 是确定没问题的提交
pub fn start(dir: &Path, bad: &str, good: &str) -> Result<String, Error> {
    git(dir, &["bisect", "start", bad, good, "--"]).map(friendly)
}

/// 标记一个提交（为空时标记当前检出的提交）。verdict：good、bad、skip
pub fn mark(dir: &Path, verdict: &str, oid: Option<&str>) -> Result<String, Error> {
    if !matches!(verdict, "good" | "bad" | "skip") {
        return Err(tr!("不支持的标记", "Unsupported mark").into());
    }
    let mut args = vec!["bisect", verdict];
    args.extend(oid);
    git(dir, &args).map(friendly)
}

/// 结束二分查错，回到开始前的分支
pub fn reset(dir: &Path) -> Result<String, Error> {
    git(dir, &["bisect", "reset"])?;
    Ok(String::new())
}

/// 把 git 的输出变成一句提示
fn friendly(out: String) -> String {
    // Bisecting: 12 revisions left to test after this (roughly 4 steps)
    if let Some(rest) = out.lines().find_map(|l| l.strip_prefix("Bisecting: ")) {
        let left: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let steps = rest
            .split("roughly ")
            .nth(1)
            .map(|s| s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>())
            .unwrap_or_default();
        return tr!(
            "已检出下一个要测试的提交：还剩 {left} 个提交，大约 {steps} 步。测试后标记它是好的还是坏的",
            "Checked out the next commit to test: {left} left, roughly {steps} steps. Test it, then mark it good or bad"
        );
    }
    if let Some(line) = out.lines().find(|l| l.contains("is the first bad commit")) {
        let oid = line.split_whitespace().next().unwrap_or("");
        return tr!("找到了：第一个有问题的提交是 {}", "Found it: the first bad commit is {}", &oid[..oid.len().min(10)]);
    }
    String::new()
}
