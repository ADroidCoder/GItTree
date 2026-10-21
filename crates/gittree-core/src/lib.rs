pub mod conflict;
pub mod diff;
pub mod flow;
pub mod graph;
pub mod history;
pub mod log;
pub mod ops;
pub mod rebase;
pub mod undo;

pub use git2;
pub use gix;

use std::sync::atomic::{AtomicBool, Ordering};

/// 提示信息用英文还是中文，界面启动时设置
static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn set_english(en: bool) {
    ENGLISH.store(en, Ordering::Relaxed);
}

pub fn english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// 按界面语言选一句提示：`tr!("分支 {name} 不存在", "Branch {name} doesn't exist")`
#[macro_export]
macro_rules! tr {
    ($zh:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::english() { format!($en $(, $arg)*) } else { format!($zh $(, $arg)*) }
    };
}
