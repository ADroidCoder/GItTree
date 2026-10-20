//! 读取整个仓库的提交历史，并保存成紧凑的内存结构。
//!
//! 为了打开大仓库时不卡：
//! - 拓扑结构（父子关系、提交时间）优先从 Git 的 commit-graph 文件读取，不用解压提交对象；
//! - 作者和提交说明是按需读取的：先读屏幕上能看到的行，其余在后台多线程补全。

use crate::graph::{self, Graph, MISSING};
use gix::ObjectId;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::path::Path;
use std::time::{Duration, Instant};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefInfo {
    /// 完整名称，例如 refs/heads/main
    pub full: String,
    /// 显示名称，例如 main、origin/main、v1.0
    pub name: String,
    pub kind: RefKind,
    pub oid: String,
    /// 指向的提交在列表中的行号，没加载到时为 None
    pub row: Option<u32>,
    pub is_head: bool,
    /// 本地分支跟踪的远程分支完整名称，例如 refs/remotes/origin/main
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeadInfo {
    /// 当前分支名，分离头指针时为 None
    pub branch: Option<String>,
    pub oid: Option<String>,
    pub row: Option<u32>,
}

/// 提交的显示信息。author == NOT_LOADED 表示还没读取。
#[derive(Debug, Clone)]
pub struct Meta {
    pub author: u32,
    pub author_time: i64,
    pub summary: Box<str>,
}

pub const NOT_LOADED: u32 = u32::MAX;

/// 后台线程读出的显示信息，作者还没去重
pub struct RawMeta {
    pub row: u32,
    pub author_name: Box<str>,
    pub author_email: Box<str>,
    pub author_time: i64,
    pub summary: Box<str>,
}

/// 所有提交的紧凑存储：每个字段一个数组，按行号索引。
pub struct Log {
    pub oids: Vec<ObjectId>,
    /// 提交时间（committer time）
    pub commit_times: Vec<i64>,
    pub meta: Vec<Meta>,
    pub meta_missing: usize,
    /// 父提交行号，CSR 格式：第 r 行的父提交是 parents[parent_start[r]..parent_start[r+1]]
    pub parent_start: Vec<u32>,
    pub parents: Vec<u32>,
    /// 作者去重表（作者名, 邮箱）
    pub author_table: Vec<(Box<str>, Box<str>)>,
    author_ids: HashMap<(Box<str>, Box<str>), u32>,
    pub row_of: HashMap<ObjectId, u32>,
    pub graph: Graph,
    pub refs: Vec<RefInfo>,
    /// 每行挂着哪些引用（refs 中的下标）
    pub refs_at: HashMap<u32, Vec<u32>>,
    pub head: HeadInfo,
    pub timings: Timings,
    /// 加载时所有引用指向的提交，用来判断刷新时是否需要重新加载历史
    pub tips: Vec<ObjectId>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Timings {
    pub read_ms: f64,
    pub sort_ms: f64,
    pub graph_ms: f64,
    pub refs_ms: f64,
    /// 从 commit-graph 文件读到的提交数 / 需要解压对象才能读到的提交数
    pub from_commit_graph: u32,
    pub from_objects: u32,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

pub fn open_gix(path: &Path) -> Result<gix::Repository, Error> {
    let mut repo = gix::open(path)?;
    repo.object_cache_size_if_unset(32 * 1024 * 1024);
    Ok(repo)
}

/// 图里要显示哪些提交的起点，排序去重。
/// focus 为空时是所有引用（不含 stash）和 HEAD；否则只用 focus 里列出的引用，
/// 例如 ["HEAD"] 表示只看当前分支，["refs/heads/main", "refs/remotes/origin/main"] 表示只看这两个分支。
pub fn read_tips(repo: &gix::Repository, focus: &[String]) -> Result<Vec<ObjectId>, Error> {
    let mut tips: Vec<ObjectId> = Vec::new();
    if !focus.is_empty() {
        for name in focus {
            let id = if name == "HEAD" {
                repo.head_id().ok().map(|id| id.detach())
            } else {
                repo.find_reference(name.as_str()).ok().and_then(|mut r| r.peel_to_id().ok()).map(|id| id.detach())
            };
            tips.extend(id);
        }
        tips.sort();
        tips.dedup();
        return Ok(tips);
    }
    for r in repo.references()?.all()?.flatten() {
        let mut r = r;
        // stash 不显示在主图里
        if r.name().as_bstr() == "refs/stash" {
            continue;
        }
        if let Ok(id) = r.peel_to_id() {
            tips.push(id.detach());
        }
    }
    if let Ok(id) = repo.head_id() {
        tips.push(id.detach());
    }
    tips.sort();
    tips.dedup();
    Ok(tips)
}

struct Node {
    oid: ObjectId,
    parents: Vec<ObjectId>,
    commit_time: i64,
    meta: Option<RawMeta>,
}

impl Log {
    pub fn len(&self) -> usize {
        self.oids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.oids.is_empty()
    }

    pub fn parents_of(&self, row: usize) -> &[u32] {
        &self.parents[self.parent_start[row] as usize..self.parent_start[row + 1] as usize]
    }

    pub fn author(&self, row: usize) -> Option<&(Box<str>, Box<str>)> {
        self.author_table.get(self.meta[row].author as usize)
    }

    pub fn load(path: &Path) -> Result<Log, Error> {
        Self::load_repo(&open_gix(path)?, &[])
    }

    /// 加载提交历史。focus 的含义见 [`read_tips`]，为空表示显示所有分支。
    pub fn load_repo(repo: &gix::Repository, focus: &[String]) -> Result<Log, Error> {
        let mut timings = Timings::default();

        // 1. 所有引用指向的提交作为起点
        let t = Instant::now();
        let tips = read_tips(repo, focus)?;

        // 2. 从起点出发找出所有可达提交
        let cg = repo.commit_graph_if_enabled().ok().flatten();
        let mut nodes: Vec<Node> = Vec::new();
        let mut index: HashMap<ObjectId, u32> = HashMap::new();
        let mut buf = Vec::new();
        let mut stack = tips.clone();
        while let Some(oid) = stack.pop() {
            if index.contains_key(&oid) {
                continue;
            }
            let node = if let Some(c) = cg.as_ref().and_then(|g| g.commit_by_id(oid)) {
                let g = cg.as_ref().unwrap();
                timings.from_commit_graph += 1;
                Node {
                    oid,
                    parents: c.iter_parents().flatten().map(|p| g.id_at(p).to_owned()).collect(),
                    commit_time: c.committer_timestamp() as i64,
                    meta: None,
                }
            } else {
                // commit-graph 之后新产生的提交，或者没有 commit-graph：解析对象本身
                let Ok(Some(data)) = gix::prelude::Find::try_find(&repo.objects, &oid, &mut buf) else {
                    continue; // 浅克隆等情况下父提交对象可能不存在
                };
                if data.kind != gix::object::Kind::Commit {
                    continue;
                }
                timings.from_objects += 1;
                let raw = parse_commit(data.data);
                Node {
                    oid,
                    parents: raw.parents.iter().filter_map(|p| ObjectId::from_hex(p.as_bytes()).ok()).collect(),
                    commit_time: raw.commit_time,
                    meta: Some(raw.to_meta(0)),
                }
            };
            stack.extend(node.parents.iter().filter(|p| !index.contains_key(*p)));
            index.insert(oid, nodes.len() as u32);
            nodes.push(node);
        }
        timings.read_ms = ms(t.elapsed());

        // 3. 拓扑排序：子提交一定排在父提交前面，其余按提交时间从新到旧（与 git log --date-order 相同）
        let t = Instant::now();
        let n = nodes.len();
        let mut pstart = Vec::with_capacity(n + 1);
        let mut pidx = Vec::with_capacity(n + n / 8);
        for node in &nodes {
            pstart.push(pidx.len() as u32);
            pidx.extend(node.parents.iter().map(|p| index.get(p).copied().unwrap_or(MISSING)));
        }
        pstart.push(pidx.len() as u32);
        let node_parents = |i: usize| &pidx[pstart[i] as usize..pstart[i + 1] as usize];
        let mut indeg = vec![0u32; n];
        for &p in &pidx {
            if p != MISSING {
                indeg[p as usize] += 1;
            }
        }
        let mut heap: BinaryHeap<(i64, Reverse<u32>)> = (0..n as u32)
            .filter(|&i| indeg[i as usize] == 0)
            .map(|i| (nodes[i as usize].commit_time, Reverse(i)))
            .collect();
        let mut row_of_node = vec![0u32; n];
        let mut order = Vec::with_capacity(n);
        while let Some((_, Reverse(i))) = heap.pop() {
            row_of_node[i as usize] = order.len() as u32;
            order.push(i);
            for &p in node_parents(i as usize) {
                if p != MISSING {
                    indeg[p as usize] -= 1;
                    if indeg[p as usize] == 0 {
                        heap.push((nodes[p as usize].commit_time, Reverse(p)));
                    }
                }
            }
        }

        let mut log = Log {
            oids: Vec::with_capacity(n),
            commit_times: Vec::with_capacity(n),
            meta: Vec::with_capacity(n),
            meta_missing: 0,
            parent_start: Vec::with_capacity(n + 1),
            parents: Vec::with_capacity(pidx.len()),
            author_table: Vec::new(),
            author_ids: HashMap::new(),
            row_of: HashMap::with_capacity(n),
            graph: Graph::default(),
            refs: Vec::new(),
            refs_at: HashMap::new(),
            head: HeadInfo { branch: None, oid: None, row: None },
            timings: Timings::default(),
            tips,
        };
        let mut pending = Vec::new();
        for &i in &order {
            let i = i as usize;
            let row = log.oids.len() as u32;
            log.parent_start.push(log.parents.len() as u32);
            log.parents.extend(node_parents(i).iter().map(|&p| {
                if p == MISSING { MISSING } else { row_of_node[p as usize] }
            }));
            let node = &mut nodes[i];
            log.row_of.insert(node.oid, row);
            log.oids.push(node.oid);
            log.commit_times.push(node.commit_time);
            log.meta.push(Meta { author: NOT_LOADED, author_time: node.commit_time, summary: "".into() });
            log.meta_missing += 1;
            if let Some(mut m) = node.meta.take() {
                m.row = row;
                pending.push(m);
            }
        }
        log.parent_start.push(log.parents.len() as u32);
        log.install_meta(pending);
        drop(nodes);
        timings.sort_ms = ms(t.elapsed());

        // 4. 计算图布局
        let t = Instant::now();
        log.graph = graph::build(n, |r| {
            &log.parents[log.parent_start[r] as usize..log.parent_start[r + 1] as usize]
        });
        timings.graph_ms = ms(t.elapsed());

        let t = Instant::now();
        log.reload_refs(repo)?;
        timings.refs_ms = ms(t.elapsed());
        log.timings = timings;
        Ok(log)
    }

    /// 重新读取分支和标签（不重新加载提交）
    pub fn reload_refs(&mut self, repo: &gix::Repository) -> Result<(), Error> {
        let (refs, head) = read_refs(repo, &self.row_of)?;
        self.refs_at.clear();
        for (i, r) in refs.iter().enumerate() {
            if let Some(row) = r.row {
                self.refs_at.entry(row).or_default().push(i as u32);
            }
        }
        self.refs = refs;
        self.head = head;
        // 领先/落后提交数用内存里的图来算，比 libgit2 解压对象快得多
        let by_full: HashMap<String, Option<u32>> = self
            .refs
            .iter()
            .filter(|r| r.kind == RefKind::Remote)
            .map(|r| (r.full.clone(), r.row))
            .collect();
        for i in 0..self.refs.len() {
            let r = &self.refs[i];
            if let (Some(row), Some(up)) = (r.row, r.upstream.as_ref())
                && let Some(Some(up_row)) = by_full.get(up)
            {
                let (a, b) = self.ahead_behind(row, *up_row);
                self.refs[i].ahead = a;
                self.refs[i].behind = b;
            }
        }
        Ok(())
    }

    /// a 有多少个提交 b 没有，b 有多少个提交 a 没有
    pub fn ahead_behind(&self, a: u32, b: u32) -> (u32, u32) {
        if a == b {
            return (0, 0);
        }
        // 行号越小越新，并且子提交行号一定小于父提交。按行号从小到大处理，
        // 处理到某行时所有能到达它的提交都已处理过，所以它的标记已经完整。
        const A: u8 = 1;
        const B: u8 = 2;
        let mut flags: HashMap<u32, u8> = HashMap::new();
        let mut heap: BinaryHeap<Reverse<u32>> = BinaryHeap::new();
        flags.insert(a, A);
        *flags.entry(b).or_insert(0) |= B;
        heap.push(Reverse(a.min(b)));
        heap.push(Reverse(a.max(b)));
        let (mut ahead, mut behind) = (0, 0);
        // 队列里还有多少行只被一边标记，归零时剩下的都是公共祖先，可以停止
        let mut single = if flags[&a] == A { 1 } else { 0 } + if flags[&b] == B { 1 } else { 0 };
        let mut last = u32::MAX;
        while let Some(Reverse(r)) = heap.pop() {
            if r == last {
                continue;
            }
            last = r;
            let f = flags[&r];
            match f {
                A => ahead += 1,
                B => behind += 1,
                _ => {}
            }
            if f != (A | B) {
                single -= 1;
            }
            for &p in self.parents_of(r as usize) {
                if p == MISSING {
                    continue;
                }
                let e = flags.entry(p).or_insert(0);
                let before = *e;
                *e |= f;
                if before == 0 {
                    heap.push(Reverse(p));
                    if *e != (A | B) {
                        single += 1;
                    }
                } else if before != (A | B) && *e == (A | B) {
                    single -= 1;
                }
            }
            if single == 0 {
                break;
            }
        }
        (ahead, behind)
    }

    /// 把读出的显示信息存进来，作者去重
    pub fn install_meta(&mut self, metas: Vec<RawMeta>) {
        for m in metas {
            let Some(slot) = self.meta.get_mut(m.row as usize) else { continue };
            if slot.author != NOT_LOADED {
                continue;
            }
            let next = self.author_table.len() as u32;
            let key = (m.author_name, m.author_email);
            let author = match self.author_ids.get(&key) {
                Some(&id) => id,
                None => {
                    self.author_table.push(key.clone());
                    self.author_ids.insert(key, next);
                    next
                }
            };
            *slot = Meta { author, author_time: m.author_time, summary: m.summary };
            self.meta_missing -= 1;
        }
    }

    /// 读取指定行里还没加载的显示信息（给当前屏幕用，几十行，很快）
    pub fn ensure_meta(&mut self, repo: &gix::Repository, rows: std::ops::Range<usize>) {
        let rows = rows.start.min(self.len())..rows.end.min(self.len());
        let missing: Vec<u32> = rows
            .filter(|&r| self.meta[r].author == NOT_LOADED)
            .map(|r| r as u32)
            .collect();
        if missing.is_empty() {
            return;
        }
        let metas = read_meta(repo, &self.oids, &missing);
        self.install_meta(metas);
    }

    /// 搜索提交说明、作者、提交哈希，返回匹配的行号（最多 limit 个）
    pub fn search(&self, query: &str, limit: usize) -> Vec<u32> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let is_hex = q.len() >= 4 && q.bytes().all(|b| b.is_ascii_hexdigit());
        let author_hit: Vec<bool> = self
            .author_table
            .iter()
            .map(|(n, e)| n.to_lowercase().contains(&q) || e.to_lowercase().contains(&q))
            .collect();
        let mut out = Vec::new();
        for r in 0..self.len() {
            let m = &self.meta[r];
            let hit = (is_hex && self.oids[r].to_string().starts_with(&q))
                || author_hit.get(m.author as usize).copied().unwrap_or(false)
                || contains_ignore_case(&m.summary, &q);
            if hit {
                out.push(r as u32);
                if out.len() >= limit {
                    break;
                }
            }
        }
        out
    }

    /// 还没加载显示信息的行
    pub fn missing_meta_rows(&self) -> Vec<u32> {
        (0..self.len() as u32).filter(|&r| self.meta[r as usize].author == NOT_LOADED).collect()
    }
}

fn contains_ignore_case(hay: &str, lower_needle: &str) -> bool {
    if hay.is_ascii() && lower_needle.is_ascii() {
        hay.as_bytes()
            .windows(lower_needle.len())
            .any(|w| w.eq_ignore_ascii_case(lower_needle.as_bytes()))
    } else {
        hay.to_lowercase().contains(lower_needle)
    }
}

/// 读取若干行的显示信息
pub fn read_meta(repo: &gix::Repository, oids: &[ObjectId], rows: &[u32]) -> Vec<RawMeta> {
    let mut buf = Vec::new();
    let mut out = Vec::with_capacity(rows.len());
    for &row in rows {
        let Some(oid) = oids.get(row as usize) else { continue };
        if let Ok(Some(data)) = gix::prelude::Find::try_find(&repo.objects, oid, &mut buf) {
            out.push(parse_commit(data.data).to_meta(row));
        }
    }
    out
}

/// 多线程读取所有行的显示信息（后台运行，用于搜索和快速滚动）
pub fn read_meta_parallel(repo: &gix::ThreadSafeRepository, oids: &[ObjectId], rows: &[u32]) -> Vec<RawMeta> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
    let chunk = rows.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = rows
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    let mut local = repo.to_thread_local();
                    local.object_cache_size_if_unset(16 * 1024 * 1024);
                    read_meta(&local, oids, part)
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

struct RawCommit<'a> {
    parents: Vec<&'a str>,
    author_name: &'a [u8],
    author_email: &'a [u8],
    author_time: i64,
    commit_time: i64,
    summary: &'a [u8],
}

impl RawCommit<'_> {
    fn to_meta(&self, row: u32) -> RawMeta {
        RawMeta {
            row,
            author_name: String::from_utf8_lossy(self.author_name).into(),
            author_email: String::from_utf8_lossy(self.author_email).into(),
            author_time: self.author_time,
            summary: String::from_utf8_lossy(self.summary).into(),
        }
    }
}

/// 直接解析提交对象的原始字节，比逐个构造 git2::Commit 快很多。
fn parse_commit(data: &[u8]) -> RawCommit<'_> {
    let mut c = RawCommit {
        parents: Vec::new(),
        author_name: b"",
        author_email: b"",
        author_time: 0,
        commit_time: 0,
        summary: b"",
    };
    let mut pos = 0;
    // 头部：每行一个字段，空行之后是提交说明
    while pos < data.len() {
        let end = memchr(b'\n', &data[pos..]).map_or(data.len(), |i| pos + i);
        let line = &data[pos..end];
        pos = end + 1;
        if line.is_empty() {
            break;
        }
        if let Some(rest) = line.strip_prefix(b"parent ") {
            if let Ok(s) = std::str::from_utf8(rest) {
                c.parents.push(s);
            }
        } else if let Some(rest) = line.strip_prefix(b"author ") {
            let (name, email, time) = parse_signature(rest);
            c.author_name = name;
            c.author_email = email;
            c.author_time = time;
        } else if let Some(rest) = line.strip_prefix(b"committer ") {
            c.commit_time = parse_signature(rest).2;
        }
        // 其他头部（tree、committer、gpgsig 的续行等）忽略
    }
    if pos < data.len() {
        let body = &data[pos..];
        let start = body.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(body.len());
        let body = &body[start..];
        let end = memchr(b'\n', body).unwrap_or(body.len());
        c.summary = &body[..end];
        if c.summary.ends_with(b"\r") {
            c.summary = &c.summary[..c.summary.len() - 1];
        }
    }
    c
}

/// 解析 `Name <email> 1700000000 +0800`
fn parse_signature(s: &[u8]) -> (&[u8], &[u8], i64) {
    let (Some(lt), Some(gt)) = (memchr(b'<', s), memrchr(b'>', s)) else {
        return (s, b"", 0);
    };
    let name = &s[..lt];
    let name = name.strip_suffix(b" ").unwrap_or(name);
    let email = &s[lt + 1..gt.max(lt + 1)];
    let time = s[(gt + 1).min(s.len())..]
        .split(|&b| b == b' ')
        .find(|t| !t.is_empty())
        .and_then(|t| std::str::from_utf8(t).ok())
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);
    (name, email, time)
}

fn memchr(needle: u8, hay: &[u8]) -> Option<usize> {
    hay.iter().position(|&b| b == needle)
}

fn memrchr(needle: u8, hay: &[u8]) -> Option<usize> {
    hay.iter().rposition(|&b| b == needle)
}

fn read_refs(
    repo: &gix::Repository,
    row_of: &HashMap<ObjectId, u32>,
) -> Result<(Vec<RefInfo>, HeadInfo), Error> {
    let head_ref = repo.head()?;
    let head_branch = head_ref
        .referent_name()
        .and_then(|n| n.as_bstr().strip_prefix(b"refs/heads/").map(|b| String::from_utf8_lossy(b).into_owned()));
    let head_oid = repo.head_id().ok().map(|id| id.detach());
    let head = HeadInfo {
        branch: head_branch.clone(),
        oid: head_oid.map(|o| o.to_string()),
        row: head_oid.and_then(|o| row_of.get(&o).copied()),
    };
    let config = repo.config_snapshot();

    let mut refs = Vec::new();
    for r in repo.references()?.all()?.flatten() {
        let mut r = r;
        let full = r.name().as_bstr().to_string();
        let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
            (RefKind::Local, n.to_string())
        } else if let Some(n) = full.strip_prefix("refs/remotes/") {
            // origin/HEAD 这种符号引用不单独显示
            if n.ends_with("/HEAD") {
                continue;
            }
            (RefKind::Remote, n.to_string())
        } else if let Some(n) = full.strip_prefix("refs/tags/") {
            (RefKind::Tag, n.to_string())
        } else {
            continue;
        };
        let Ok(id) = r.peel_to_id() else { continue };
        let oid = id.detach();
        let upstream = if kind == RefKind::Local {
            let remote = config.string(format!("branch.{name}.remote").as_str());
            let merge = config.string(format!("branch.{name}.merge").as_str());
            match (remote, merge) {
                (Some(remote), Some(merge)) => {
                    let merge = merge.to_string();
                    let short = merge.strip_prefix("refs/heads/").unwrap_or(&merge);
                    Some(format!("refs/remotes/{remote}/{short}"))
                }
                _ => None,
            }
        } else {
            None
        };
        refs.push(RefInfo {
            is_head: kind == RefKind::Local && head_branch.as_deref() == Some(name.as_str()),
            full,
            name,
            kind,
            oid: oid.to_string(),
            row: row_of.get(&oid).copied(),
            upstream,
            ahead: 0,
            behind: 0,
        });
    }
    Ok((refs, head))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_commit() {
        let raw = "tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
parent 1111111111111111111111111111111111111111\n\
parent 2222222222222222222222222222222222222222\n\
author Zhang San <zs@example.com> 1700000000 +0800\n\
committer Li Si <ls@example.com> 1700000100 +0800\n\
\n\
\n  修复：图表滚动卡顿\r\n\nbody\n";
        let c = parse_commit(raw.as_bytes());
        assert_eq!(c.parents.len(), 2);
        assert_eq!(c.author_name, b"Zhang San");
        assert_eq!(c.author_email, b"zs@example.com");
        assert_eq!(c.author_time, 1700000000);
        assert_eq!(c.commit_time, 1700000100);
        assert_eq!(std::str::from_utf8(c.summary).unwrap(), "修复：图表滚动卡顿");
    }
}
