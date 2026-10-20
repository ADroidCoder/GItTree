// Windows 发布版不显示控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    gittree_lib::run()
}
