//! 提交身份（user.name / user.email）：查看当前仓库用的是哪个，保存几套常用的身份，
//! 按目录自动切换。
//!
//! 每套身份是一个小的 git 配置文件；按目录切换用的是 git 自带的 includeIf：
//! 在全局配置里写 `[includeIf "gitdir:~/work/"] path = <身份文件>`，
//! 这样在命令行和其他工具里提交也会用对的身份，GitTree 没打开也一样。

use crate::tr;
use crate::log::Error;
use crate::ops::git;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Default)]
pub struct Identity {
    pub name: Option<String>,
    pub email: Option<String>,
    /// email 来自哪里：local（这个仓库）、global、system、command
    pub scope: Option<String>,
    /// email 来自哪个配置文件
    pub origin: Option<String>,
}

/// 仓库提交时实际会用的身份
pub fn current(dir: &Path) -> Identity {
    let get = |key: &str| git(dir, &["config", "--get", key]).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let mut id = Identity { name: get("user.name"), email: get("user.email"), ..Default::default() };
    // global\tfile:/Users/a/.gitconfig\ta@b.com
    if let Ok(out) = git(dir, &["config", "--show-scope", "--show-origin", "--get", "user.email"]) {
        let mut p = out.trim_end().splitn(3, '\t');
        id.scope = p.next().map(str::to_string);
        id.origin = p.next().map(|o| o.strip_prefix("file:").unwrap_or(o).to_string());
    }
    id
}

/// 全局配置（~/.gitconfig）里的身份，不受当前目录和目录规则影响
pub fn global(dir: &Path) -> Identity {
    let get = |key: &str| git(dir, &["config", "--global", "--get", key]).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    Identity { name: get("user.name"), email: get("user.email"), scope: Some("global".into()), origin: None }
}

/// 设置全局身份；留空的项从全局配置里删掉
pub fn set_global(dir: &Path, name: &str, email: &str) -> Result<(), Error> {
    for (key, value) in [("user.name", name.trim()), ("user.email", email.trim())] {
        if value.is_empty() {
            let _ = git(dir, &["config", "--global", "--unset-all", key]);
        } else {
            git(dir, &["config", "--global", key, value])?;
        }
    }
    Ok(())
}

/// 只给这个仓库设置身份（写到 .git/config）
pub fn set_local(dir: &Path, name: &str, email: &str) -> Result<(), Error> {
    git(dir, &["config", "--local", "user.name", name.trim()])?;
    git(dir, &["config", "--local", "user.email", email.trim()])?;
    Ok(())
}

/// 去掉这个仓库单独的设置，改用全局 / 目录规则里的身份
pub fn clear_local(dir: &Path) -> Result<(), Error> {
    for key in ["user.name", "user.email"] {
        // 本来就没设置时 git 返回 5，不算错
        let _ = git(dir, &["config", "--local", "--unset-all", key]);
    }
    Ok(())
}

#[derive(Debug, Serialize, Clone)]
pub struct Profile {
    pub id: String,
    pub label: String,
    pub name: String,
    pub email: String,
    pub signing_key: Option<String>,
    /// 在这些目录下的仓库自动用这个身份
    pub dirs: Vec<String>,
    /// 身份文件的完整路径
    pub file: String,
}

fn file_of(store: &Path, id: &str) -> Result<PathBuf, Error> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(tr!("身份编号不对", "Invalid profile id").into());
    }
    Ok(store.join(format!("{id}.gitconfig")))
}

/// git 配置里的路径统一用 /
fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// 全局配置里所有的目录规则：(gitdir 条件, 指向的文件)
fn rules(store: &Path) -> Vec<(String, String)> {
    let out = git(store, &["config", "--global", "-z", "--get-regexp", r"^includeif\.gitdir"]).unwrap_or_default();
    out.split('\0')
        .filter_map(|rec| {
            let (key, value) = rec.split_once('\n')?;
            // includeif.gitdir:/Users/a/work/.path（小节名里的大小写会保留）
            let cond = key.strip_prefix("includeif.")?.strip_suffix(".path")?;
            Some((cond.to_string(), value.to_string()))
        })
        .collect()
}

/// gitdir:/a/b/ → /a/b
fn dir_of_cond(cond: &str) -> Option<String> {
    let d = cond.strip_prefix("gitdir:").or_else(|| cond.strip_prefix("gitdir/i:"))?;
    Some(d.trim_end_matches('/').to_string())
}

fn cond_of_dir(dir: &str) -> String {
    let d = dir.trim().replace('\\', "/");
    let d = d.trim_end_matches('/');
    // Windows 的路径不区分大小写
    if cfg!(windows) { format!("gitdir/i:{d}/") } else { format!("gitdir:{d}/") }
}

pub fn profiles(store: &Path) -> Result<Vec<Profile>, Error> {
    std::fs::create_dir_all(store)?;
    let rules = rules(store);
    let mut list = Vec::new();
    for entry in std::fs::read_dir(store)? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "gitconfig") {
            continue;
        }
        let id = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let f = path.to_string_lossy().into_owned();
        let get = |key: &str| git(store, &["config", "-f", &f, "--get", key]).ok().map(|s| s.trim().to_string());
        let file = slash(&path);
        list.push(Profile {
            label: get("gittree.label").unwrap_or_default(),
            name: get("user.name").unwrap_or_default(),
            email: get("user.email").unwrap_or_default(),
            signing_key: get("user.signingkey").filter(|s| !s.is_empty()),
            dirs: dirs_of(&rules, &file),
            file,
            id,
        });
    }
    list.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(list)
}

/// 一套身份的目录（去掉 set_dirs 额外写的真实路径）
fn dirs_of(rules: &[(String, String)], file: &str) -> Vec<String> {
    let all: Vec<String> = rules.iter().filter(|(_, v)| same_path(v, file)).filter_map(|(c, _)| dir_of_cond(c)).collect();
    let mut out: Vec<String> = Vec::new();
    for d in &all {
        let real = std::fs::canonicalize(d).map(|r| dunce(&r.to_string_lossy()).replace('\\', "/"));
        let dup = out.iter().any(|o| {
            let o_real = std::fs::canonicalize(o).map(|r| dunce(&r.to_string_lossy()).replace('\\', "/"));
            matches!((&real, &o_real), (Ok(a), Ok(b)) if a == b)
        });
        if !dup {
            out.push(d.clone());
        }
    }
    out
}

fn same_path(a: &str, b: &str) -> bool {
    let n = |s: &str| s.replace('\\', "/");
    if cfg!(windows) { n(a).eq_ignore_ascii_case(&n(b)) } else { n(a) == n(b) }
}

/// 新建（id 为空）或修改一套身份，返回它的编号
pub fn save_profile(store: &Path, id: Option<&str>, label: &str, name: &str, email: &str, signing_key: Option<&str>) -> Result<String, Error> {
    if name.trim().is_empty() || email.trim().is_empty() {
        return Err(tr!("请填写用户名和邮箱", "Enter a name and an email").into());
    }
    std::fs::create_dir_all(store)?;
    let id = match id.filter(|i| !i.is_empty()) {
        Some(i) => i.to_string(),
        None => format!("p{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()),
    };
    let file = file_of(store, &id)?;
    let f = file.to_string_lossy().into_owned();
    let label = if label.trim().is_empty() { email.trim() } else { label.trim() };
    git(store, &["config", "-f", &f, "gittree.label", label])?;
    git(store, &["config", "-f", &f, "user.name", name.trim()])?;
    git(store, &["config", "-f", &f, "user.email", email.trim()])?;
    match signing_key.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => git(store, &["config", "-f", &f, "user.signingkey", k]).map(|_| ())?,
        None => {
            let _ = git(store, &["config", "-f", &f, "--unset", "user.signingkey"]);
        }
    }
    Ok(id)
}

/// 设置哪些目录下的仓库自动用这套身份（会修改全局的 ~/.gitconfig）
pub fn set_dirs(store: &Path, id: &str, dirs: &[String]) -> Result<(), Error> {
    let file = slash(&file_of(store, id)?);
    remove_rules(store, &file)?;
    let mut conds: Vec<String> = Vec::new();
    for d in dirs.iter().map(|d| d.trim()).filter(|d| !d.is_empty()) {
        conds.push(cond_of_dir(d));
        // 目录经过符号链接时（例如 macOS 的 /var → /private/var），git 按真实路径匹配，两个都写上
        if let Ok(real) = std::fs::canonicalize(d) {
            let real = cond_of_dir(&dunce(&real.to_string_lossy()));
            if !conds.contains(&real) {
                conds.push(real);
            }
        }
    }
    for c in conds {
        git(store, &["config", "--global", "--add", &format!("includeIf.{c}.path"), &file])?;
    }
    Ok(())
}

/// Windows 上 canonicalize 的结果带 \\?\ 前缀
fn dunce(p: &str) -> String {
    p.strip_prefix(r"\\?\").filter(|r| !r.starts_with("UNC")).unwrap_or(p).to_string()
}

fn remove_rules(store: &Path, file: &str) -> Result<(), Error> {
    for (cond, value) in rules(store) {
        if same_path(&value, file) {
            let key = format!("includeIf.{cond}.path");
            git(store, &["config", "--global", "--fixed-value", "--unset-all", &key, &value])?;
        }
    }
    Ok(())
}

pub fn delete_profile(store: &Path, id: &str) -> Result<(), Error> {
    let path = file_of(store, id)?;
    remove_rules(store, &slash(&path))?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitdir_conditions() {
        let c = cond_of_dir("/Users/a/my work/");
        assert_eq!(dir_of_cond(&c).as_deref(), Some("/Users/a/my work"));
        assert_eq!(dir_of_cond("gitdir/i:C:/code/").as_deref(), Some("C:/code"));
        assert!(dir_of_cond("onbranch:main").is_none());
    }
}
