//! 补丁：把提交导出成 .patch（git format-patch），或者把别人发来的补丁应用到当前分支。
//!
//! 补丁有两种：format-patch 导出的邮件格式带作者和提交说明，用 git am 应用后直接变成提交；
//! git diff 输出的普通差异只有改动，用 git apply 放到工作区，检查后自己提交。

use crate::log::Error;
use crate::ops::{git, git_with_input};
use crate::tr;
use std::path::Path;

/// 把提交按给出的顺序导出成一段文本，可以直接粘贴给别人。合并提交没有补丁，会被跳过
pub fn export_text(dir: &Path, oids: &[String]) -> Result<String, Error> {
    let mut out = String::new();
    for oid in oids {
        if !is_merge(dir, oid)? {
            out.push_str(&git(dir, &["format-patch", "-1", "--stdout", oid])?);
        }
    }
    if out.is_empty() {
        return Err(no_patch());
    }
    Ok(out)
}

/// 每个提交导出成 out 下的一个文件（0001-说明.patch 这样编号），返回文件路径
pub fn export_files(dir: &Path, oids: &[String], out: &Path) -> Result<Vec<String>, Error> {
    let out_dir = out.to_string_lossy();
    let mut files = Vec::new();
    for oid in oids {
        if is_merge(dir, oid)? {
            continue;
        }
        let n = (files.len() + 1).to_string();
        let printed = git(dir, &["format-patch", "-1", "--start-number", &n, "-o", &out_dir, oid])?;
        files.extend(printed.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string));
    }
    if files.is_empty() {
        return Err(no_patch());
    }
    Ok(files)
}

/// format-patch -1 遇到合并提交会跳过它、导出更早的一个，所以先排除掉
fn is_merge(dir: &Path, oid: &str) -> Result<bool, Error> {
    Ok(git(dir, &["rev-list", "--parents", "-n", "1", oid])?.split_whitespace().count() > 2)
}

fn no_patch() -> Error {
    tr!("合并提交没有可以导出的补丁", "Merge commits can't be exported as patches").into()
}

/// format-patch 导出的邮件格式：第一行是 “From <提交哈希> Mon Sep 17 00:00:00 2001”
pub fn is_mail(data: &str) -> bool {
    let first = data.trim_start().lines().next().unwrap_or("");
    first
        .strip_prefix("From ")
        .and_then(|rest| rest.split(' ').next())
        .is_some_and(|h| (h.len() == 40 || h.len() == 64) && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// 应用补丁：files 是补丁文件，text 是粘贴进来的内容（两者给一个）。
/// 邮件格式用 git am 逐个变成提交，冲突时停下等用户解决后继续；普通差异放到工作区
pub fn apply(dir: &Path, files: &[String], text: Option<&str>) -> Result<String, Error> {
    let contents: Vec<String> = match text {
        Some(t) => vec![t.to_string()],
        None => files.iter().map(std::fs::read_to_string).collect::<Result<_, _>>()?,
    };
    if contents.iter().all(|c| c.trim().is_empty()) {
        return Err(tr!("补丁是空的", "The patch is empty").into());
    }
    let mail = contents.iter().filter(|c| is_mail(c)).count();
    if mail != 0 && mail != contents.len() {
        return Err(tr!(
            "这些文件里既有 format-patch 导出的补丁，也有普通差异，请分开应用",
            "Some files are format-patch mails and some are plain diffs. Apply them separately"
        )
        .into());
    }
    let run = |args: &[&str]| -> Result<String, Error> {
        let mut args = args.to_vec();
        args.extend(files.iter().map(String::as_str));
        git_with_input(dir, &args, text.map(str::as_bytes))
    };
    if mail > 0 {
        // -3：补丁对不上时用三方合并，冲突的地方留给用户解决
        return run(&["am", "-3"]);
    }
    // 先直接应用；对不上再三方合并，留下冲突
    run(&["apply", "--whitespace=nowarn"]).or_else(|_| run(&["apply", "--3way", "--whitespace=nowarn"]))?;
    Ok(tr!("补丁里的改动已经放到工作区，检查后再提交", "The changes are in your working tree. Review them, then commit"))
}
