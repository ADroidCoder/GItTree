//! 撤销：执行会移动分支的操作前，记下所有本地分支、标签和 HEAD 指向哪里；
//! 撤销时把变化过的引用改回去。被“丢掉”的提交在 reflog 里还在，所以可以恢复。
//!
//! 为了不误伤之后的改动，撤销前会检查这些引用是否还停在操作后的位置，
//! 如果在那之后又提交或切换过（包括在命令行里），就拒绝撤销。

use crate::tr;
use crate::log::Error;
use crate::ops::git;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// HEAD 指向的分支（refs/heads/xxx），分离 HEAD 时为空
    head_ref: Option<String>,
    head_oid: Option<String>,
    /// 本地分支和标签 → 提交（或附注标签对象）
    refs: BTreeMap<String, String>,
}

pub fn snapshot(dir: &Path) -> Result<Snapshot, Error> {
    let out = git(dir, &["for-each-ref", "--format=%(refname) %(objectname)", "refs/heads", "refs/tags"])?;
    let refs = out
        .lines()
        .filter_map(|l| l.split_once(' '))
        .map(|(r, o)| (r.to_string(), o.to_string()))
        .collect();
    let head_ref = git(dir, &["symbolic-ref", "-q", "HEAD"]).ok().map(|s| s.trim().to_string());
    let head_oid = git(dir, &["rev-parse", "-q", "--verify", "HEAD"]).ok().map(|s| s.trim().to_string());
    Ok(Snapshot { head_ref, head_oid, refs })
}

/// 怎样把当前分支移回去
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 提交、修改提交：撤销后改动回到暂存区
    Soft,
    /// 合并、变基、重置等：工作区里没提交的改动保留，冲突时拒绝
    Keep,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub label: String,
    mode: Mode,
    before: Snapshot,
    after: Snapshot,
}

/// 操作前后的快照有变化时生成一条撤销记录
pub fn record(label: impl Into<String>, mode: Mode, before: Snapshot, after: Snapshot) -> Option<Entry> {
    (before != after).then(|| Entry { label: label.into(), mode, before, after })
}

impl Entry {
    /// 仓库是否还停在这次操作之后的状态（否则这条记录已经过期）
    pub fn is_current(&self, dir: &Path) -> Result<bool, Error> {
        let now = snapshot(dir)?;
        Ok(now.head_ref == self.after.head_ref
            && now.head_oid == self.after.head_oid
            && self.changed_refs().iter().all(|r| now.refs.get(*r) == self.after.refs.get(*r)))
    }

    fn changed_refs(&self) -> BTreeSet<&String> {
        let keys: BTreeSet<&String> = self.before.refs.keys().chain(self.after.refs.keys()).collect();
        keys.into_iter().filter(|k| self.before.refs.get(*k) != self.after.refs.get(*k)).collect()
    }
}

fn short(r: &str) -> &str {
    r.strip_prefix("refs/heads/").or_else(|| r.strip_prefix("refs/tags/")).unwrap_or(r)
}

fn set_ref(dir: &Path, name: &str, oid: Option<&String>) -> Result<(), Error> {
    match oid {
        Some(o) => git(dir, &["update-ref", "-m", "gittree: 撤销", name, o])?,
        None => git(dir, &["update-ref", "-d", name])?,
    };
    Ok(())
}

/// 撤销一条记录，返回给用户看的说明
pub fn undo(dir: &Path, git_dir: &Path, e: &Entry) -> Result<String, Error> {
    if git_dir.join("MERGE_HEAD").exists()
        || git_dir.join("rebase-merge").exists()
        || git_dir.join("rebase-apply").exists()
        || git_dir.join("CHERRY_PICK_HEAD").exists()
        || git_dir.join("REVERT_HEAD").exists()
    {
        return Err(tr!("还有没完成的合并 / 变基，请先完成或中止后再撤销", "A merge or rebase is in progress. Finish or abort it before undoing").into());
    }
    let now = snapshot(dir)?;
    let changed = e.changed_refs();
    for r in &changed {
        if now.refs.get(*r) != e.after.refs.get(*r) {
            return Err(tr!("无法撤销“{}”：{} 在那之后又变化了", "Can't undo “{}”: {} has changed since then", e.label, short(r)).into());
        }
    }
    if now.head_ref != e.after.head_ref || now.head_oid != e.after.head_oid {
        return Err(tr!("无法撤销“{}”：在那之后又切换过分支或有了新提交", "Can't undo “{}”: the branch was switched or new commits were made since then", e.label).into());
    }

    // 1. 先把 HEAD 恢复到原来的分支 / 提交
    let before_head = e.before.head_ref.as_ref();
    if e.before.head_ref != e.after.head_ref {
        match before_head {
            Some(b) => {
                // 原来所在的分支此时没有被检出，可以直接改回原位置再切换过去
                if now.refs.get(b) != e.before.refs.get(b) {
                    set_ref(dir, b, e.before.refs.get(b))?;
                }
                git(dir, &["checkout", short(b)])?;
            }
            None => {
                let oid = e.before.head_oid.as_deref().ok_or_else(|| tr!("原来的 HEAD 不存在", "The original HEAD no longer exists"))?;
                git(dir, &["checkout", "--detach", oid])?;
            }
        }
    } else if e.before.head_oid != e.after.head_oid {
        match (&e.before.head_oid, before_head) {
            (Some(oid), Some(_)) => {
                let mode = if e.mode == Mode::Soft { "--soft" } else { "--keep" };
                git(dir, &["reset", mode, oid])?;
            }
            (Some(oid), None) => {
                git(dir, &["checkout", "--detach", oid])?;
            }
            // 撤销仓库里的第一个提交：删掉分支，改动留在暂存区
            (None, Some(b)) => set_ref(dir, b, None)?,
            (None, None) => {}
        }
    }

    // 2. 其他分支和标签直接改回去（新建的删掉，删除的恢复）
    for r in changed {
        if Some(r) == before_head {
            continue;
        }
        set_ref(dir, r, e.before.refs.get(r))?;
    }
    Ok(tr!("已撤销：{}", "Undone: {}", e.label))
}
