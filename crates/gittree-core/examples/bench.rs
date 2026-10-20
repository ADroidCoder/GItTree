//! 用法：cargo run --release -p gittree-core --example bench -- <仓库路径>
use gittree_core::log::{self, Log};
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let t = Instant::now();
    let mut log = Log::load(path.as_ref()).expect("打开仓库失败");
    let total = t.elapsed();
    println!("提交数: {}  引用数: {}  最大泳道数: {}  连线数: {}",
        log.len(), log.refs.len(), log.graph.max_lanes, log.graph.edges.len());
    println!("{:?}", log.timings);
    println!("打开仓库总耗时: {:.1} ms", total.as_secs_f64() * 1000.0);

    let repo = log::open_gix(path.as_ref()).unwrap();
    let t = Instant::now();
    log.ensure_meta(&repo, 40000..40060);
    println!("读取一屏 60 行的作者和说明: {:.2} ms", t.elapsed().as_secs_f64() * 1000.0);

    let t = Instant::now();
    let rows = log.missing_meta_rows();
    let metas = log::read_meta_parallel(&repo.into_sync(), &log.oids, &rows);
    let n = metas.len();
    log.install_meta(metas);
    println!("后台多线程读取全部 {} 行说明: {:.1} ms（剩余未读 {}）", n, t.elapsed().as_secs_f64() * 1000.0, log.meta_missing);

    let t = Instant::now();
    let n = log.len() as u32;
    let mut sum = 0;
    let mut x: u32 = 12345;
    for _ in 0..10_000 {
        x = x.wrapping_mul(1103515245).wrapping_add(12345);
        let s = x % n.max(1);
        sum += log.graph.edges_in(s, s + 60).len();
    }
    println!("滚动区间查询: 平均 {:.2} µs / 次，平均每屏 {} 条连线",
        t.elapsed().as_secs_f64() * 1e6 / 10_000.0, sum / 10_000);
}
