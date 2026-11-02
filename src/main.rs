#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod desktop;
#[cfg(all(test, not(windows)))]
#[path = "desktop/swim.rs"]
mod swim;
fn main() {
    #[cfg(windows)]
    desktop::run();
    #[cfg(not(windows))]
    eprintln!("遁鱼桌面程序仅支持 Windows；此平台可运行核心测试。");
}
