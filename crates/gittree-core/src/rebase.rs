//! 交互式变基：界面上排好顺序、选好保留 / 合并 / 删除、改好提交说明，
//! 这里生成 git 的待办列表，用 GIT_SEQUENCE_EDITOR 替换掉 git 自己生成的那份。
//! 修改提交说明用 `exec git commit --amend -F 文件`，全程不需要打开编辑器。

use crate::tr;
use crate::log::Error;
use crate::ops::{git, git_env};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 一次最多处理这么多个提交
const MAX_COMMITS: usize = 500;

#[derive(Debug, Serialize)]
pub struct PlanCommit {
    pub oid: String,
    pub parents: usize,
    pub author: String,
    pub time: i64,
    pub summary: String,
    /// 完整的提交说明
    pub message: String,
}

/// 从 from（包含）到 HEAD 沿第一父提交的所有提交，旧的在前
pub fn plan(dir: &Path, from: &str) -> Result<Vec<PlanCommit>, Error> {
    if git(dir, &["merge-base", "--is-ancestor", from, "HEAD"]).is_err() {
        return Err(tr!("这个提交不在当前分支的历史上，只能从当前分支的提交开始交互式变基", "This commit isn't in the current branch's history. Interactive rebase must start from a commit on the current branch").into());
    }
    let range = if has_parent(dir, from) { format!("{from}^..HEAD") } else { "HEAD".into() };
    let out = git(dir, &["log", "--first-parent", "--reverse", "--format=%H%x1f%P%x1f%an%x1f%at%x1f%B%x1e", &range])?;
    let list: Vec<PlanCommit> = out
        .split('\x1e')
        .filter_map(|rec| {
            let f: Vec<&str> = rec.trim_start_matches('\n').splitn(5, '\x1f').collect();
            if f.len() < 5 {
                return None;
            }
            let message = f[4].trim_end().to_string();
            Some(PlanCommit {
                oid: f[0].to_string(),
                parents: f[1].split_whitespace().count(),
                author: f[2].to_string(),
                time: f[3].parse().unwrap_or(0),
                summary: message.lines().next().unwrap_or("").to_string(),
                message,
            })
        })
        .collect();
    if list.len() > MAX_COMMITS {
        return Err(tr!("一次最多变基 {MAX_COMMITS} 个提交，请选择更近的起点（现在是 {} 个）", "At most {MAX_COMMITS} commits can be rebased at once; pick a more recent starting point (currently {})", list.len()).into());
    }
    Ok(list)
}

fn has_parent(dir: &Path, oid: &str) -> bool {
    git(dir, &["rev-parse", "-q", "--verify", &format!("{oid}^")]).is_ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Pick,
    /// 合并到前一个保留的提交里
    Squash,
    Drop,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    pub oid: String,
    pub action: Action,
    /// 新的提交说明（对 pick 有效，包括合并了后面几个提交之后的说明），为空表示不改
    #[serde(default)]
    pub message: Option<String>,
}

/// 给 sh 用的单引号字符串
fn sh_quote(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 按 steps 的顺序执行交互式变基（steps 旧的在前）
pub fn run(dir: &Path, git_dir: &Path, from: &str, steps: &[Step]) -> Result<String, Error> {
    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        return Err(tr!("已经有一个变基在进行中，请先继续或中止", "A rebase is already in progress. Continue or abort it first").into());
    }
    if steps.iter().find(|s| s.action != Action::Drop).is_some_and(|s| s.action == Action::Squash) {
        return Err(tr!("第一个保留的提交不能合并到前一个提交", "The first kept commit can't be squashed into a previous one").into());
    }
    if steps.iter().all(|s| s.action == Action::Drop) {
        return Err(tr!("至少要保留一个提交", "Keep at least one commit").into());
    }
    let work = git_dir.join("gittree-rebase");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    let mut todo = String::new();
    // 每组（一个 pick 加上后面合并进来的提交）结束后，需要改说明就加一行 exec
    let mut pending: Option<String> = None;
    let flush = |todo: &mut String, pending: &mut Option<String>, n: usize| -> Result<(), Error> {
        if let Some(msg) = pending.take() {
            let file = work.join(format!("msg-{n}.txt"));
            std::fs::write(&file, msg.trim_end().to_string() + "\n")?;
            todo.push_str(&format!("exec git commit --amend --allow-empty -q -F {}\n", sh_quote(&file)));
        }
        Ok(())
    };
    for (i, s) in steps.iter().enumerate() {
        match s.action {
            Action::Drop => todo.push_str(&format!("drop {}\n", s.oid)),
            Action::Pick => {
                flush(&mut todo, &mut pending, i)?;
                todo.push_str(&format!("pick {}\n", s.oid));
                pending = s.message.clone().filter(|m| !m.trim().is_empty());
            }
            Action::Squash => todo.push_str(&format!("fixup {}\n", s.oid)),
        }
    }
    flush(&mut todo, &mut pending, steps.len())?;

    let todo_file = work.join("todo");
    std::fs::write(&todo_file, &todo)?;
    // git 会在命令后面加上待办文件的路径：cp 我们的列表 覆盖 它
    let editor = format!("cp {}", sh_quote(&todo_file));
    let base = if has_parent(dir, from) { format!("{from}^") } else { String::new() };
    let mut args = vec!["rebase", "-i", "--autostash"];
    if base.is_empty() {
        args.push("--root");
    } else {
        args.push(&base);
    }
    git_env(dir, &args, &[("GIT_SEQUENCE_EDITOR", &editor)]).map_err(|e| {
        if git_dir.join("rebase-merge").exists() {
            tr!("变基停在了冲突上。请解决冲突后点“继续”，或者点“中止”回到变基前。\n\n{e}", "The rebase stopped on a conflict. Resolve it and click “Continue”, or click “Abort” to go back.\n\n{e}").into()
        } else {
            e
        }
    })
}
