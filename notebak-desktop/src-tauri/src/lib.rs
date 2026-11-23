// lib.rs —— note&back 桌面端入口：卡片首页 + 笔记窗口（内嵌 fnote-web）

mod backtool;
mod java_server;
mod pyexec;

use std::sync::atomic::{AtomicBool, Ordering};

use java_server::{JavaServer, LaunchError, ServerState};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Manager, State, Url, WebviewUrl};

struct AppServerState {
    server: JavaServer,
    closing: AtomicBool,
}

// 启动 Java 服务
#[tauri::command]
fn start_server(
    app: tauri::AppHandle,
    state: State<'_, AppServerState>,
) -> Result<(), String> {
    // 首页是 MPA，从其他页面返回会重新执行 start_server，已在运行则直接返回
    if state.server.is_running() {
        return Ok(());
    }
    let resource_dir = app.path().resource_dir().ok();
    match state.server.start(app, resource_dir) {
        Ok(_) => Ok(()),
        Err(LaunchError::JavaNotFound) => Err(
            "找不到 Java：打包后应位于 App 内 jre/，开发态需设置 JAVA_HOME".to_string(),
        ),
        Err(LaunchError::JarNotFound) => {
            Err("找不到 fnote.jar，请先在项目根目录执行 mvn package".to_string())
        }
        Err(LaunchError::LaunchFailed(msg)) => Err(format!("Java 进程启动失败：{}", msg)),
    }
}

// 停止 Java 服务
#[tauri::command]
fn stop_server(state: State<'_, AppServerState>) {
    state.server.stop_and_wait(6);
}

// 重启 Java 服务
#[tauri::command]
fn restart_server(
    app: tauri::AppHandle,
    state: State<'_, AppServerState>,
) -> Result<(), String> {
    state.server.stop_and_wait(4);
    start_server(app, state)
}

// 读取已缓存的服务日志（日志窗口打开前的启动日志）
#[tauri::command]
fn get_server_log(app: tauri::AppHandle) -> Vec<String> {
    app.try_state::<ServerState>()
        .map(|state| state.logs.lock().unwrap().clone())
        .unwrap_or_default()
}

// Java 服务当前监听地址
fn notes_url(app: &tauri::AppHandle) -> Result<Url, String> {
    let port = app
        .try_state::<ServerState>()
        .and_then(|state| *state.port.lock().unwrap())
        .ok_or_else(|| "本地服务尚未就绪，请稍候再试".to_string())?;
    Url::parse(&format!("http://localhost:{}/", port)).map_err(|e| format!("地址无效：{}", e))
}

// 打开笔记（fnote-web）窗口，已存在则聚焦
fn open_notes_window(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("notes") {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }
    let url = notes_url(app)?;
    tauri::WebviewWindowBuilder::new(app, "notes", WebviewUrl::External(url))
        .title("note&back 笔记")
        .inner_size(1280.0, 820.0)
        .min_inner_size(960.0, 620.0)
        .build()
        .map_err(|e| format!("打开笔记窗口失败：{}", e))?;
    Ok(())
}

#[tauri::command]
fn open_notes(app: tauri::AppHandle) -> Result<(), String> {
    open_notes_window(&app)
}

// 打开内嵌 BackTool（独立 app，自带 runtime）
#[tauri::command]
fn open_backtool(app: tauri::AppHandle) -> Result<(), String> {
    backtool::start(app)
}

// 打开日志窗口
#[tauri::command]
fn open_log_window(app: tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("log") {
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    tauri::WebviewWindowBuilder::new(&app, "log", WebviewUrl::App("log.html".into()))
        .title("服务日志")
        .inner_size(720.0, 480.0)
        .build()
        .ok();
}

// 关闭流程：隐藏窗口 → 后台线程优雅停服 → 退出进程。
// 停服（SIGINT + 等待）不能放在主线程，否则界面转圈卡死。
fn shutdown_and_exit(app: tauri::AppHandle) {
    let already_closing = app
        .try_state::<AppServerState>()
        .map(|state| state.closing.swap(true, Ordering::SeqCst))
        .unwrap_or(false);
    if already_closing {
        return; // 已在关闭流程中
    }
    for label in ["main", "notes", "log"] {
        if let Some(wv) = app.get_webview_window(label) {
            let _ = wv.hide();
        }
    }
    std::thread::spawn(move || {
        backtool::stop(2);
        if let Some(state) = app.try_state::<AppServerState>() {
            state.server.stop_and_wait(6);
        }
        app.exit(0);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppServerState {
            server: JavaServer::new(),
            closing: AtomicBool::new(false),
        })
        .manage(ServerState::default())
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            restart_server,
            get_server_log,
            open_notes,
            open_backtool,
            open_log_window,
            pyexec::pyexec_list,
            pyexec::pyexec_run,
            pyexec::pick_conf_file
        ])
        .setup(|app| {
            // 构建菜单（第一个子菜单是 macOS 的应用菜单，提供 ⌘Q 退出）
            let app_menu = Submenu::new(app, "note&back", true)?;
            app_menu.append(&MenuItem::with_id(app, "quit", "退出 note&back", true, Some("CmdOrCtrl+Q"))?)?;
            let nav = Submenu::new(app, "功能", true)?;
            nav.append(&MenuItem::with_id(app, "notes", "笔记", true, Some("CmdOrCtrl+1"))?)?;
            nav.append(&MenuItem::with_id(app, "backtool", "备份小工具", true, Some("CmdOrCtrl+2"))?)?;
            nav.append(&MenuItem::with_id(app, "home", "首页", true, Some("CmdOrCtrl+0"))?)?;
            nav.append(&PredefinedMenuItem::separator(app)?)?;
            nav.append(&MenuItem::with_id(app, "log", "服务日志…", true, Some("CmdOrCtrl+L"))?)?;
            let menu = Menu::with_items(app, &[&app_menu, &nav])?;
            app.set_menu(menu)?;

            // 首页窗口关闭即退出整个 App（异步停服，避免卡住界面）
            let app_handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        shutdown_and_exit(app_handle.clone());
                    }
                });
            }

            // 菜单事件
            let app_handle = app.handle().clone();
            app.on_menu_event(move |_win, event| {
                let app = app_handle.clone();
                match event.id().as_ref() {
                    "quit" => shutdown_and_exit(app),
                    "notes" => {
                        let _ = open_notes_window(&app);
                    }
                    "backtool" => {
                        let _ = backtool::start(app);
                    }
                    "home" => {
                        if let Some(wv) = app.get_webview_window("main") {
                            let _ = wv.show();
                            let _ = wv.set_focus();
                        }
                    }
                    "log" => open_log_window(app),
                    _ => {}
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        // ⌘Q / Dock 退出同样要先停掉 Java 子进程，否则会残留后台服务。
        // 关窗按钮的流程已经会调 exit，此时 closing 已为 true，这里直接放行。
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let has_child = app_handle
                    .try_state::<AppServerState>()
                    .map(|state| state.server.is_running())
                    .unwrap_or(false)
                    || backtool::is_running();
                if has_child {
                    api.prevent_exit();
                    shutdown_and_exit(app_handle.clone());
                }
            }
        });
}
