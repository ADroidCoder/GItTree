//! 提交图的泳道（lane）布局。
//!
//! 输入是按拓扑顺序排好的提交（子提交一定在父提交之前），以及每个提交的父提交行号。
//! 输出每个提交所在的列，以及所有父子连线。整个布局一次性在 O(提交数 + 连线数) 内算完，
//! 之后界面按可见区域查询，不管仓库多大，每次只需要处理屏幕上那几十行。

use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// 父提交不在已加载范围内（例如浅克隆）时使用的行号。
pub const MISSING: u32 = u32::MAX;

const NO_LANE: u16 = u16::MAX;

/// 每隔多少行保存一次“正在穿过此处的连线”快照，用来快速查询任意区间。
const BLOCK: u32 = 256;

/// 一条从子提交连到父提交的线。
///
/// 线的走向：从 (child_row, child_col) 出发，若 lane 与 child_col 不同，就在下一行之前弯到 lane；
/// 沿 lane 竖直向下；若 lane 与 parent_col 不同，就在父提交的前一行弯到 parent_col。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Edge {
    pub child_row: u32,
    pub parent_row: u32,
    pub child_col: u16,
    pub parent_col: u16,
    pub lane: u16,
    pub color: u8,
}

#[derive(Debug, Default)]
pub struct Graph {
    pub node_col: Vec<u16>,
    pub node_color: Vec<u8>,
    pub edges: Vec<Edge>,
    /// checkpoints[k] = 在第 k*BLOCK 行开始之前就已出现、并且还没结束的连线编号。
    checkpoints: Vec<Vec<u32>>,
    pub max_lanes: u16,
}

pub const PALETTE_SIZE: u8 = 10;

struct Lanes {
    free: BinaryHeap<Reverse<u16>>,
    next: u16,
    color: Vec<u8>,
    next_color: u8,
}

impl Lanes {
    fn alloc(&mut self) -> u16 {
        let lane = match self.free.pop() {
            Some(Reverse(l)) => l,
            None => {
                let l = self.next;
                self.next = self.next.saturating_add(1).min(NO_LANE - 1);
                self.color.push(0);
                l
            }
        };
        self.color[lane as usize] = self.next_color;
        self.next_color = (self.next_color + 1) % PALETTE_SIZE;
        lane
    }

    fn release(&mut self, lane: u16) {
        self.free.push(Reverse(lane));
    }
}

/// `parents(row)` 返回该行提交的父提交行号（父行号必须大于子行号，或为 MISSING）。
pub fn build<'a, F>(rows: usize, parents: F) -> Graph
where
    F: Fn(usize) -> &'a [u32],
{
    let mut g = Graph {
        node_col: vec![0; rows],
        node_color: vec![0; rows],
        edges: Vec::with_capacity(rows + rows / 8),
        checkpoints: Vec::with_capacity(rows / BLOCK as usize + 1),
        max_lanes: 0,
    };
    let mut lanes = Lanes {
        free: BinaryHeap::new(),
        next: 0,
        color: Vec::new(),
        next_color: 0,
    };
    // waiting[p] = 已经为父提交 p 预留好的主泳道，p 最终就画在这一列。
    let mut waiting = vec![NO_LANE; rows];
    // 除主泳道外，还有哪些泳道也在等 p（多个分支从同一个提交分叉出来）。到达 p 时释放。
    let mut extra: HashMap<u32, Vec<u16>> = HashMap::new();
    let mut active: Vec<u32> = Vec::new();

    for r in 0..rows {
        if r as u32 % BLOCK == 0 {
            active.retain(|&e| g.edges[e as usize].parent_row >= r as u32);
            g.checkpoints.push(active.clone());
        }

        let col = match waiting[r] {
            NO_LANE => lanes.alloc(),
            l => l,
        };
        if let Some(ls) = extra.remove(&(r as u32)) {
            for l in ls {
                lanes.release(l);
            }
        }
        g.node_col[r] = col;
        g.node_color[r] = lanes.color[col as usize];

        let mut col_kept = false;
        for (i, &p) in parents(r).iter().enumerate() {
            if p == MISSING || p as usize >= rows {
                continue;
            }
            debug_assert!(p as usize > r, "提交必须按拓扑顺序排列");
            let pw = waiting[p as usize];
            let (lane, parent_col) = if i == 0 && !col_kept {
                if pw == NO_LANE {
                    // 最常见的情况：沿当前列直接往下走
                    waiting[p as usize] = col;
                    (col, col)
                } else {
                    // 父提交已经被别的分支占了一列：本分支保持在自己这一列，到父提交前再并过去，
                    // 这样能看清楚分叉点在哪里
                    extra.entry(p).or_default().push(col);
                    (col, pw)
                }
            } else if pw == NO_LANE {
                // 合并提交的其他父提交，开一条新泳道
                let l = lanes.alloc();
                waiting[p as usize] = l;
                (l, l)
            } else {
                (pw, pw)
            };
            if lane == col {
                col_kept = true;
            }
            active.push(g.edges.len() as u32);
            g.edges.push(Edge {
                child_row: r as u32,
                parent_row: p,
                child_col: col,
                parent_col,
                lane,
                color: lanes.color[lane as usize],
            });
        }
        if !col_kept {
            lanes.release(col);
        }
    }
    g.max_lanes = lanes.next;
    g
}

impl Graph {
    /// 返回与 [start, end) 行区间有交集的所有连线（编号, 连线）。
    pub fn edges_in(&self, start: u32, end: u32) -> Vec<(u32, Edge)> {
        let mut out = Vec::new();
        if self.checkpoints.is_empty() || start >= end {
            return out;
        }
        let block = ((start / BLOCK) as usize).min(self.checkpoints.len() - 1);
        let block_start = block as u32 * BLOCK;
        for &id in &self.checkpoints[block] {
            let e = self.edges[id as usize];
            if e.parent_row >= start {
                out.push((id, e));
            }
        }
        // edges 按 child_row 递增排列，二分找到区块起点
        let first = self.edges.partition_point(|e| e.child_row < block_start);
        for (i, e) in self.edges[first..].iter().enumerate() {
            if e.child_row >= end {
                break;
            }
            if e.parent_row >= start {
                out.push(((first + i) as u32, *e));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(parents: &[Vec<u32>]) -> Graph {
        build(parents.len(), |r| parents[r].as_slice())
    }

    #[test]
    fn linear_history_uses_one_lane() {
        let g = layout(&[vec![1], vec![2], vec![]]);
        assert_eq!(g.node_col, vec![0, 0, 0]);
        assert_eq!(g.max_lanes, 1);
        assert_eq!(g.edges.len(), 2);
    }

    #[test]
    fn merge_opens_and_closes_a_lane() {
        // 0 = merge(1, 2); 1 -> 3; 2 -> 3
        let g = layout(&[vec![1, 2], vec![3], vec![3], vec![]]);
        assert_eq!(g.node_col, vec![0, 0, 1, 0]);
        assert_eq!(g.max_lanes, 2);
        let e = g.edges.iter().find(|e| e.child_row == 2).unwrap();
        assert_eq!((e.lane, e.parent_col), (1, 0));
    }

    #[test]
    fn sibling_branch_tips_keep_their_own_column_until_fork_point() {
        // 两个分支头 0 和 1，共同父提交 2
        let g = layout(&[vec![2], vec![2], vec![]]);
        assert_eq!(g.node_col, vec![0, 1, 0]);
        let e = g.edges.iter().find(|e| e.child_row == 1).unwrap();
        assert_eq!((e.child_col, e.lane, e.parent_col), (1, 1, 0));
    }

    #[test]
    fn range_query_matches_brute_force() {
        // 构造一个有大量分叉合并的图
        let n = 3000usize;
        let parents: Vec<Vec<u32>> = (0..n)
            .map(|r| {
                let mut p = vec![];
                if r + 1 < n {
                    p.push((r + 1) as u32);
                }
                if r % 7 == 0 && r + 40 < n {
                    p.push((r + 40) as u32);
                }
                if r % 13 == 0 && r + 900 < n {
                    p.push((r + 900) as u32);
                }
                p
            })
            .collect();
        let g = layout(&parents);
        for (s, e) in [(0, 10), (250, 300), (255, 257), (1000, 1600), (2990, 3000)] {
            let mut got: Vec<u32> = g.edges_in(s, e).into_iter().map(|(i, _)| i).collect();
            got.sort();
            let want: Vec<u32> = g
                .edges
                .iter()
                .enumerate()
                .filter(|(_, x)| x.child_row < e && x.parent_row >= s)
                .map(|(i, _)| i as u32)
                .collect();
            assert_eq!(got, want, "区间 {s}..{e}");
        }
    }
}
