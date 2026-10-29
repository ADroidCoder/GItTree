//! 冲突解决：解析工作区文件里的冲突标记，按用户对每一处冲突的选择生成结果。
//!
//! 全程按原始字节处理，GBK 等非 UTF-8 文件选择“当前 / 传入”时不会被改坏；
//! 只有 UTF-8 文件才允许手动编辑结果。

use crate::tr;
use crate::log::Error;
use crate::ops::git;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 解析后的一段内容
enum Raw {
    Common(Vec<u8>),
    Conflict {
        ours: Vec<u8>,
        base: Option<Vec<u8>>,
        theirs: Vec<u8>,
        ours_label: String,
        theirs_label: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Chunk {
    Common { text: String },
    Conflict { ours: String, base: Option<String>, theirs: String, ours_label: String, theirs_label: String },
}

#[derive(Debug, Serialize)]
pub struct ConflictFile {
    pub path: String,
    pub chunks: Vec<Chunk>,
    pub conflicts: usize,
    /// 是 UTF-8 文本，可以手动编辑
    pub utf8: bool,
    pub binary: bool,
    /// 文件太大没有逐处解析（字节数），只能整个选一边
    pub too_large: Option<u64>,
    /// “当前”一侧有这个文件（没有说明这一侧删除了它）
    pub ours_exists: bool,
    pub theirs_exists: bool,
}

fn marker<'a>(line: &'a [u8], ch: u8) -> Option<&'a [u8]> {
    if line.len() >= 7 && line[..7].iter().all(|&b| b == ch) {
        let rest = &line[7..];
        if rest.is_empty() || rest[0] == b' ' || rest[0] == b'\n' || rest[0] == b'\r' {
            return Some(rest);
        }
    }
    None
}

fn label(rest: &[u8]) -> String {
    String::from_utf8_lossy(rest).trim().to_string()
}

fn parse(data: &[u8]) -> Vec<Raw> {
    #[derive(PartialEq)]
    enum St {
        Normal,
        Ours,
        Base,
        Theirs,
    }
    let mut out = Vec::new();
    let mut common = Vec::new();
    let (mut ours, mut base, mut theirs) = (Vec::new(), None::<Vec<u8>>, Vec::new());
    let mut ours_label = String::new();
    let mut st = St::Normal;
    for line in data.split_inclusive(|&b| b == b'\n') {
        match st {
            St::Normal => {
                if let Some(rest) = marker(line, b'<') {
                    if !common.is_empty() {
                        out.push(Raw::Common(std::mem::take(&mut common)));
                    }
                    ours_label = label(rest);
                    st = St::Ours;
                } else {
                    common.extend_from_slice(line);
                }
            }
            St::Ours | St::Base => {
                if marker(line, b'|').is_some() && st == St::Ours {
                    base = Some(Vec::new());
                    st = St::Base;
                } else if marker(line, b'=').is_some() {
                    st = St::Theirs;
                } else if st == St::Ours {
                    ours.extend_from_slice(line);
                } else {
                    base.as_mut().unwrap().extend_from_slice(line);
                }
            }
            St::Theirs => {
                if let Some(rest) = marker(line, b'>') {
                    out.push(Raw::Conflict {
                        ours: std::mem::take(&mut ours),
                        base: base.take(),
                        theirs: std::mem::take(&mut theirs),
                        ours_label: std::mem::take(&mut ours_label),
                        theirs_label: label(rest),
                    });
                    st = St::Normal;
                } else {
                    theirs.extend_from_slice(line);
                }
            }
        }
    }
    // 标记不完整：剩下的内容原样当作普通内容
    if st != St::Normal {
        let mut rest = format!("<<<<<<< {ours_label}\n").into_bytes();
        rest.extend(ours);
        if let Some(b) = base {
            rest.extend_from_slice(b"|||||||\n");
            rest.extend(b);
        }
        if st == St::Theirs {
            rest.extend_from_slice(b"=======\n");
            rest.extend(theirs);
        }
        common.extend(rest);
    }
    if !common.is_empty() {
        out.push(Raw::Common(common));
    }
    out
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// 冲突文件各个阶段是否存在：2 = 当前（ours），3 = 传入（theirs）
fn stages(dir: &Path, path: &str) -> Result<(bool, bool), Error> {
    let out = git(dir, &["ls-files", "-u", "-z", "--", path])?;
    let mut ours = false;
    let mut theirs = false;
    for entry in out.split('\0') {
        // 格式：<mode> <oid> <stage>\t<path>
        match entry.split('\t').next().and_then(|m| m.split(' ').nth(2)) {
            Some("2") => ours = true,
            Some("3") => theirs = true,
            _ => {}
        }
    }
    Ok((ours, theirs))
}

/// 超过这么大的冲突文件不逐处解析，界面上几十 MB 的内容会卡住
const MAX_PARSE_SIZE: u64 = 2 * 1024 * 1024;

/// 看起来是二进制：开头有 NUL，或者控制字符很多。
/// 没有 NUL 的二进制文件 git 也会当成文本合并，插进去成百上千处冲突标记。
/// GBK 等编码的中文是 0x80 以上的字节，不算控制字符
fn looks_binary(data: &[u8]) -> bool {
    let head = &data[..data.len().min(64 * 1024)];
    if head.iter().take(8000).any(|&b| b == 0) {
        return true;
    }
    let control = head.iter().filter(|&&b| (b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r' | 0x0c)) || b == 0x7f).count();
    control * 20 > head.len()
}

pub fn load(dir: &Path, path: &str) -> Result<ConflictFile, Error> {
    let (ours_exists, theirs_exists) = stages(dir, path)?;
    let size = std::fs::metadata(dir.join(path)).map(|m| m.len()).unwrap_or(0);
    if size > MAX_PARSE_SIZE {
        return Ok(ConflictFile {
            path: path.to_string(),
            chunks: Vec::new(),
            conflicts: 0,
            utf8: false,
            binary: true,
            too_large: Some(size),
            ours_exists,
            theirs_exists,
        });
    }
    let data = std::fs::read(dir.join(path)).unwrap_or_default();
    let binary = looks_binary(&data);
    let utf8 = std::str::from_utf8(&data).is_ok();
    let raw = if binary { Vec::new() } else { parse(&data) };
    let mut conflicts = 0;
    let chunks = raw
        .iter()
        .map(|r| match r {
            Raw::Common(b) => Chunk::Common { text: text(b) },
            Raw::Conflict { ours, base, theirs, ours_label, theirs_label } => {
                conflicts += 1;
                Chunk::Conflict {
                    ours: text(ours),
                    base: base.as_deref().map(text),
                    theirs: text(theirs),
                    ours_label: ours_label.clone(),
                    theirs_label: theirs_label.clone(),
                }
            }
        })
        .collect();
    Ok(ConflictFile { path: path.to_string(), chunks, conflicts, utf8, binary, too_large: None, ours_exists, theirs_exists })
}

/// 用户对一处冲突的选择
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "pick", rename_all = "snake_case")]
pub enum Choice {
    Ours,
    Theirs,
    Base,
    /// 两边都要，当前在前
    OursTheirs,
    /// 两边都要，传入在前
    TheirsOurs,
    /// 手动编辑的内容（只有 UTF-8 文件可以）
    Text { text: String },
}

/// 按选择写出结果并标记为已解决
pub fn resolve(dir: &Path, path: &str, choices: &[Choice]) -> Result<(), Error> {
    let file = dir.join(path);
    let data = std::fs::read(&file)?;
    let raw = parse(&data);
    let count = raw.iter().filter(|r| matches!(r, Raw::Conflict { .. })).count();
    if count != choices.len() {
        return Err(tr!("文件内容已经变化，请重新打开后再解决", "The file has changed. Reopen it and try again").into());
    }
    let utf8 = std::str::from_utf8(&data).is_ok();
    let mut out = Vec::with_capacity(data.len());
    let mut picks = choices.iter();
    for r in raw {
        match r {
            Raw::Common(b) => out.extend(b),
            Raw::Conflict { ours, base, theirs, .. } => match picks.next().unwrap() {
                Choice::Ours => out.extend(ours),
                Choice::Theirs => out.extend(theirs),
                Choice::Base => out.extend(base.unwrap_or_default()),
                Choice::OursTheirs => {
                    out.extend(ours);
                    out.extend(theirs);
                }
                Choice::TheirsOurs => {
                    out.extend(theirs);
                    out.extend(ours);
                }
                Choice::Text { text } => {
                    if !utf8 {
                        return Err(tr!("这个文件不是 UTF-8 编码，不能手动编辑，请选择当前或传入的内容", "This file isn't UTF-8 and can't be edited by hand. Choose the current or incoming content").into());
                    }
                    out.extend_from_slice(text.as_bytes());
                }
            },
        }
    }
    std::fs::write(&file, out)?;
    git(dir, &["add", "--", path])?;
    Ok(())
}

/// 整个文件使用某一边的版本；那一边删除了文件时就删除它
pub fn resolve_side(dir: &Path, path: &str, theirs: bool) -> Result<(), Error> {
    let (ours_exists, theirs_exists) = stages(dir, path)?;
    let exists = if theirs { theirs_exists } else { ours_exists };
    if exists {
        git(dir, &["checkout", if theirs { "--theirs" } else { "--ours" }, "--", path])?;
        git(dir, &["add", "--", path])?;
    } else {
        git(dir, &["rm", "-q", "--", path])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_without_nul_is_detected() {
        // 没有 NUL 但控制字符很多的二进制数据
        let bin: Vec<u8> = (0..20_000u32).map(|i| (i * 7 % 255 + 1) as u8).collect();
        assert!(looks_binary(&bin));
        // GBK 中文（“中文”= D6 D0 CE C4）和普通文本不算
        assert!(!looks_binary(&b"\xD6\xD0\xCE\xC4\n".repeat(1000)));
        assert!(!looks_binary(b"fn main() {\n\tprintln!();\n}\n"));
    }

    #[test]
    fn parse_markers_with_base() {
        let src = b"a\n<<<<<<< HEAD\nours\n||||||| base\norig\n=======\ntheirs\n>>>>>>> feature\nz\n";
        let raw = parse(src);
        assert_eq!(raw.len(), 3);
        match &raw[1] {
            Raw::Conflict { ours, base, theirs, ours_label, theirs_label } => {
                assert_eq!(ours, b"ours\n");
                assert_eq!(base.as_deref(), Some(&b"orig\n"[..]));
                assert_eq!(theirs, b"theirs\n");
                assert_eq!(ours_label, "HEAD");
                assert_eq!(theirs_label, "feature");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn unterminated_marker_is_kept() {
        let src = b"a\n<<<<<<< HEAD\nx\n";
        let raw = parse(src);
        let all: Vec<u8> = raw
            .into_iter()
            .flat_map(|r| match r {
                Raw::Common(b) => b,
                _ => panic!(),
            })
            .collect();
        assert_eq!(all, src);
    }
}
