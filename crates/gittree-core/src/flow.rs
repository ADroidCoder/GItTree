//! Git Flow 工作流（feature / bugfix / release / hotfix），直接用 git 命令实现，
//! 不需要安装 git-flow 工具。配置保存在仓库的 git config 里，键名和 git-flow (AVH) 相同，
//! 所以和命令行的 git flow、GitKraken、SourceTree 互通。
//!
//! 完成（finish）操作的每一步都会先检查是否已经做过，遇到冲突中断后，
//! 用户解决冲突并提交，再执行一次完成就会从中断的地方继续。

use crate::tr;
use crate::log::Error;
use crate::ops::git;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowConfig {
    /// 生产分支，通常是 main 或 master
    pub master: String,
    pub develop: String,
    pub feature: String,
    pub bugfix: String,
    pub release: String,
    pub hotfix: String,
    /// 版本标签前缀，例如 "v"
    pub version_tag: String,
    /// 仓库是否已经初始化过 Git Flow
    pub initialized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Feature,
    Bugfix,
    Release,
    Hotfix,
}

impl Kind {
    fn prefix<'a>(&self, cfg: &'a FlowConfig) -> &'a str {
        match self {
            Kind::Feature => &cfg.feature,
            Kind::Bugfix => &cfg.bugfix,
            Kind::Release => &cfg.release,
            Kind::Hotfix => &cfg.hotfix,
        }
    }

    /// 默认从哪个分支开始
    fn base<'a>(&self, cfg: &'a FlowConfig) -> &'a str {
        match self {
            Kind::Hotfix => &cfg.master,
            _ => &cfg.develop,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Kind::Feature => "feature",
            Kind::Bugfix => "bugfix",
            Kind::Release => "release",
            Kind::Hotfix => "hotfix",
        }
    }
}

fn branch_exists(dir: &Path, name: &str) -> bool {
    git(dir, &["rev-parse", "--verify", "-q", &format!("refs/heads/{name}")]).is_ok()
}

fn tag_exists(dir: &Path, name: &str) -> bool {
    git(dir, &["rev-parse", "--verify", "-q", &format!("refs/tags/{name}")]).is_ok()
}

/// a 是否已经包含在 b 里（a 是 b 的祖先）
fn is_merged(dir: &Path, a: &str, b: &str) -> bool {
    git(dir, &["merge-base", "--is-ancestor", a, b]).is_ok()
}

/// 读取 Git Flow 配置；没有初始化时返回推荐的默认值。
/// 每次刷新都会调用，所以只运行一次 git config。
pub fn config(dir: &Path) -> FlowConfig {
    let out = git(dir, &["config", "--get-regexp", r"^gitflow\."]).unwrap_or_default();
    let values: std::collections::HashMap<&str, &str> = out
        .lines()
        .map(|l| l.split_once(' ').unwrap_or((l, "")))
        .collect();
    let initialized = values.contains_key("gitflow.branch.develop");
    let or = |key: &str, d: &str| values.get(key).filter(|v| !v.is_empty()).unwrap_or(&d).to_string();
    let master = match values.get("gitflow.branch.master") {
        Some(m) if !m.is_empty() => m.to_string(),
        // 没配置时，主分支优先用已存在的 main，其次 master
        _ if !branch_exists(dir, "main") && branch_exists(dir, "master") => "master".into(),
        _ => "main".into(),
    };
    FlowConfig {
        master,
        develop: or("gitflow.branch.develop", "develop"),
        feature: or("gitflow.prefix.feature", "feature/"),
        bugfix: or("gitflow.prefix.bugfix", "bugfix/"),
        release: or("gitflow.prefix.release", "release/"),
        hotfix: or("gitflow.prefix.hotfix", "hotfix/"),
        // 版本标签前缀允许为空
        version_tag: values.get("gitflow.prefix.versiontag").unwrap_or(&"").to_string(),
        initialized,
    }
}

fn check_name(name: &str) -> Result<(), Error> {
    if name.trim().is_empty() {
        return Err(tr!("名称不能为空", "Name can't be empty").into());
    }
    if name.chars().any(|c| c.is_whitespace() || "~^:?*[\\".contains(c)) || name.contains("..") {
        return Err(tr!("“{name}” 不能用作分支名（不能包含空格和 ~ ^ : ? * [ \\ ..）", "“{name}” isn't a valid branch name (no spaces or ~ ^ : ? * [ \\ ..)").into());
    }
    Ok(())
}

pub fn init(dir: &Path, cfg: &FlowConfig) -> Result<(), Error> {
    check_name(&cfg.master)?;
    check_name(&cfg.develop)?;
    if cfg.master == cfg.develop {
        return Err(tr!("生产分支和开发分支不能是同一个", "The production and develop branches must be different").into());
    }
    if !branch_exists(dir, &cfg.master) {
        return Err(tr!("生产分支 {} 不存在，请先创建它（至少需要一个提交）", "The production branch {} doesn't exist. Create it first (it needs at least one commit)", cfg.master).into());
    }
    for (key, value) in [
        ("gitflow.branch.master", &cfg.master),
        ("gitflow.branch.develop", &cfg.develop),
        ("gitflow.prefix.feature", &cfg.feature),
        ("gitflow.prefix.bugfix", &cfg.bugfix),
        ("gitflow.prefix.release", &cfg.release),
        ("gitflow.prefix.hotfix", &cfg.hotfix),
        ("gitflow.prefix.versiontag", &cfg.version_tag),
    ] {
        git(dir, &["config", key, value])?;
    }
    // support 分支界面上不用，但命令行 git flow 需要这个配置；别的工具已经设置过就保留原值
    if git(dir, &["config", "--get", "gitflow.prefix.support"]).is_err() {
        git(dir, &["config", "gitflow.prefix.support", "support/"])?;
    }
    if !branch_exists(dir, &cfg.develop) {
        // 远程已有 develop 时从远程创建，否则从生产分支创建
        let remote = format!("origin/{}", cfg.develop);
        if git(dir, &["rev-parse", "--verify", "-q", &format!("refs/remotes/{remote}")]).is_ok() {
            git(dir, &["branch", "--track", &cfg.develop, &remote])?;
        } else {
            git(dir, &["branch", &cfg.develop, &cfg.master])?;
        }
    }
    Ok(())
}

fn require_initialized(dir: &Path) -> Result<FlowConfig, Error> {
    let cfg = config(dir);
    if !cfg.initialized {
        return Err(tr!("这个仓库还没有初始化 Git Flow", "Git Flow isn't initialized in this repository").into());
    }
    Ok(cfg)
}

/// 开始一个 feature / bugfix / release / hotfix 分支并切换过去
pub fn start(dir: &Path, kind: Kind, name: &str, base: Option<&str>) -> Result<String, Error> {
    let cfg = require_initialized(dir)?;
    check_name(name)?;
    let branch = format!("{}{}", kind.prefix(&cfg), name.trim());
    if branch_exists(dir, &branch) {
        return Err(tr!("分支 {branch} 已经存在", "Branch {branch} already exists").into());
    }
    if matches!(kind, Kind::Release | Kind::Hotfix) {
        let tag = format!("{}{}", cfg.version_tag, name.trim());
        if tag_exists(dir, &tag) {
            return Err(tr!("版本标签 {tag} 已经存在，请换一个版本号", "Tag {tag} already exists. Choose another version").into());
        }
    }
    let base = base.filter(|b| !b.trim().is_empty()).unwrap_or(kind.base(&cfg));
    git(dir, &["switch", "-c", &branch, base])?;
    Ok(branch)
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FinishOptions {
    /// 完成后保留分支（默认删除）
    #[serde(default)]
    pub keep_branch: bool,
    /// release / hotfix 的标签说明，为空时用 “Release 1.2.0” 这样的默认说明
    #[serde(default)]
    pub tag_message: Option<String>,
    /// 完成后推送到 origin
    #[serde(default)]
    pub push: bool,
}

fn ensure_clean(dir: &Path, git_dir: &Path) -> Result<(), Error> {
    if git_dir.join("MERGE_HEAD").exists() {
        return Err(tr!("还有没完成的合并，请先解决冲突并提交，然后再点一次“完成”", "A merge is still in progress. Resolve the conflicts and commit, then click “Finish” again").into());
    }
    let out = git(dir, &["status", "--porcelain", "--untracked-files=no"])?;
    if !out.trim().is_empty() {
        return Err(tr!("有未提交的更改，请先提交或 stash 后再完成", "You have uncommitted changes. Commit or stash them before finishing").into());
    }
    Ok(())
}

/// 把 source（分支或标签）合并到 target，已经合并过就跳过
fn merge_into(dir: &Path, source: &str, target: &str, is_tag: bool, steps: &mut Vec<String>) -> Result<(), Error> {
    if is_merged(dir, source, target) {
        steps.push(tr!("{source} 已经包含在 {target} 里，跳过合并", "{source} is already in {target}, merge skipped"));
        return Ok(());
    }
    git(dir, &["switch", target])?;
    let msg = format!("Merge {} '{source}' into {target}", if is_tag { "tag" } else { "branch" });
    if let Err(e) = git(dir, &["merge", "--no-ff", "-m", &msg, source]) {
        return Err(tr!(
            "把 {source} 合并到 {target} 时出现冲突。\n请解决冲突并提交，然后再点一次“完成”，已完成的步骤会自动跳过。\n\n{e}",
            "Merging {source} into {target} caused conflicts.\nResolve them and commit, then click “Finish” again. Completed steps are skipped automatically.\n\n{e}"
        )
        .into());
    }
    steps.push(tr!("已把 {source} 合并到 {target}", "Merged {source} into {target}"));
    Ok(())
}

/// 完成分支：feature / bugfix 合并回 develop；release / hotfix 合并到生产分支、打标签、再合并回 develop
pub fn finish(dir: &Path, git_dir: &Path, kind: Kind, name: &str, opts: &FinishOptions) -> Result<String, Error> {
    let cfg = require_initialized(dir)?;
    let branch = format!("{}{}", kind.prefix(&cfg), name.trim());
    if !branch_exists(dir, &branch) {
        return Err(tr!("分支 {branch} 不存在", "Branch {branch} doesn't exist").into());
    }
    ensure_clean(dir, git_dir)?;
    let mut steps = Vec::new();
    let mut push_refs: Vec<String> = vec![cfg.develop.clone()];

    match kind {
        Kind::Feature | Kind::Bugfix => merge_into(dir, &branch, &cfg.develop, false, &mut steps)?,
        Kind::Release | Kind::Hotfix => {
            merge_into(dir, &branch, &cfg.master, false, &mut steps)?;
            let tag = format!("{}{}", cfg.version_tag, name.trim());
            if tag_exists(dir, &tag) {
                steps.push(tr!("标签 {tag} 已存在，跳过", "Tag {tag} already exists, skipped"));
            } else {
                let default_msg = format!("{} {}", if kind == Kind::Release { "Release" } else { "Hotfix" }, name.trim());
                let msg = opts.tag_message.as_deref().filter(|m| !m.trim().is_empty()).unwrap_or(&default_msg);
                git(dir, &["tag", "-a", &tag, &cfg.master, "-m", msg])?;
                steps.push(tr!("已创建标签 {tag}", "Created tag {tag}"));
            }
            // 和 git-flow 一样把版本标签合并回 develop，这样 develop 也包含打了标签的提交
            merge_into(dir, &tag, &cfg.develop, true, &mut steps)?;
            push_refs.push(cfg.master.clone());
            push_refs.push(format!("refs/tags/{tag}"));
        }
    }

    // 最后停在 develop 上（和 git flow 的行为一致）
    git(dir, &["switch", &cfg.develop])?;
    if !opts.keep_branch {
        git(dir, &["branch", "-d", &branch])?;
        steps.push(tr!("已删除分支 {branch}", "Deleted branch {branch}"));
    }
    if opts.push {
        let mut args = vec!["push", "--progress", "origin"];
        args.extend(push_refs.iter().map(String::as_str));
        git(dir, &args)?;
        steps.push(tr!("已推送 {}", "Pushed {}", push_refs.join(if crate::english() { ", " } else { "、" })));
    }
    Ok(tr!("{} {} 已完成：\n{}", "Finished {} {}:\n{}", kind.label(), name.trim(), steps.join("\n")))
}

/// 把分支推送到 origin 并设置跟踪，方便别人协作
pub fn publish(dir: &Path, kind: Kind, name: &str) -> Result<String, Error> {
    let cfg = require_initialized(dir)?;
    let branch = format!("{}{}", kind.prefix(&cfg), name.trim());
    git(dir, &["push", "--progress", "-u", "origin", &branch])?;
    Ok(tr!("已推送 {branch} 到 origin", "Pushed {branch} to origin"))
}
