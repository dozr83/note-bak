// main.rs —— Tauri 二进制入口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    fnote_shell_lib::run()
}
