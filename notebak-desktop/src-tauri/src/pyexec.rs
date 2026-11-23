// pyexec.rs —— 一键 Python 脚本执行器（迁移自 PlayLang4mpv）
// 用户配置路径（script.conf 完整路径）由前端 localStorage 存储并传入
// script.conf 里 script.N.name / script.N.cmd 生成按钮
// 脚本根目录 = script.conf 的父目录（.py 文件放同目录）
// 执行：python3 - + stdin 喂脚本内容 + emit 推 stdout/stderr 到前端
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::Emitter;
use tauri_plugin_dialog::DialogExt;

#[derive(serde::Serialize)]
pub struct ScriptItem {
    pub name: String,
    pub cmd: String,
}

#[derive(serde::Serialize)]
pub struct ScriptsResult {
    pub dir: String,
    pub scripts: Vec<ScriptItem>,
}

/// 读取 script.conf（完整路径由前端传入），返回脚本列表 + 脚本根目录（conf 的父目录）
#[tauri::command]
pub fn pyexec_list(conf_path: String) -> Result<ScriptsResult, String> {
    let conf = PathBuf::from(&conf_path);
    if !conf.exists() {
        return Err(format!("配置文件不存在：{}", conf.display()));
    }
    let script_root = conf.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let content = fs::read_to_string(&conf).map_err(|e| format!("读取失败：{}", e))?;

    // 解析 Properties 格式：script.N.name / script.N.cmd
    let mut map: BTreeMap<i32, (String, String)> = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = &line[..eq];
            let val = &line[eq + 1..];
            let parts: Vec<&str> = key.split('.').collect();
            if parts.len() == 3 && parts[0] == "script" {
                if let Ok(num) = parts[1].parse::<i32>() {
                    let entry = map.entry(num).or_default();
                    if parts[2] == "name" {
                        entry.0 = val.to_string();
                    } else if parts[2] == "cmd" {
                        entry.1 = val.to_string();
                    }
                }
            }
        }
    }

    let list: Vec<ScriptItem> = map
        .into_iter()
        .filter(|(_, (name, cmd))| !name.is_empty() && !cmd.is_empty())
        .map(|(_, (name, cmd))| ScriptItem { name, cmd })
        .collect();

    Ok(ScriptsResult {
        dir: script_root.to_string_lossy().to_string(),
        scripts: list,
    })
}

/// 执行脚本：python3 - + stdin 喂脚本文件 + emit 推 stdout/stderr
/// conf_path 用于定位脚本根目录（conf 的父目录）
#[tauri::command]
pub fn pyexec_run(app: tauri::AppHandle, conf_path: String, cmd: String) -> Result<(), String> {
    let conf = PathBuf::from(&conf_path);
    let scripts_root = conf.parent().map(|p| p.to_path_buf()).unwrap_or_default();

    // 解析脚本名和参数（空格切分，和 Java 原版逻辑一致）
    let (py_file_name, args) = match cmd.find(' ') {
        Some(i) => (cmd[..i].to_string(), cmd[i + 1..].to_string()),
        None => (cmd.clone(), String::new()),
    };

    let py_path = scripts_root.join(&py_file_name);
    if !py_path.exists() {
        return Err(format!("脚本不存在：{}", py_path.display()));
    }

    // 子线程跑子进程，避免阻塞 invoke
    std::thread::spawn(move || {
        let _ = run_and_emit(&app, &py_path, &args);
    });

    Ok(())
}

/// 弹出系统文件选择框，返回配置文件绝对路径（取消返回 None）
#[tauri::command]
pub async fn pick_conf_file(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .add_filter("配置文件", &["conf", "properties", "txt"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().to_string())
}

fn run_and_emit(app: &tauri::AppHandle, py_path: &PathBuf, args: &str) -> Result<(), String> {
    let mut command = Command::new("python3");
    command.arg("-"); // stdin 读脚本
    if let Some(parent) = py_path.parent() {
        command.current_dir(parent);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    // 参数按空格切分（Java 原版逻辑）
    if !args.is_empty() {
        for arg in args.split_whitespace() {
            command.arg(arg);
        }
    }

    let mut child = command.spawn().map_err(|e| format!("启动失败：{}", e))?;

    // 写 stdin（脚本内容）：独立线程写，避免 stdout 满管道死锁
    if let Some(stdin) = child.stdin.take() {
        let content = fs::read(py_path).map_err(|e| format!("读脚本失败：{}", e))?;
        std::thread::spawn(move || {
            let mut stdin = stdin;
            let _ = stdin.write_all(&content);
            // drop 时关闭 stdin，python 收到 EOF 开始执行
        });
    }

    // 读 stdout，emit 推前端
    if let Some(stdout) = child.stdout.take() {
        let app_clone = app.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(|l| l.ok()) {
                let _ = app_clone.emit("script-output", line);
            }
        });
    }

    // 读 stderr，同样 emit
    if let Some(stderr) = child.stderr.take() {
        let app_clone = app.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(|l| l.ok()) {
                let _ = app_clone.emit("script-output", line);
            }
        });
    }

    let status = child.wait().map_err(|e| format!("等待失败：{}", e))?;
    let _ = app.emit(
        "script-output",
        if status.success() {
            "=== 执行完成 ==="
        } else {
            "=== 执行失败 ==="
        },
    );

    Ok(())
}
