// java_server.rs —— 管理内嵌 Spring Boot（fnote.jar）子进程的生命周期
// 移植自 JavaServer.swift：定位 java 与 jar、随机端口启动、解析 Tomcat 日志、
// 就绪回调、退出回调、App 退出时优雅停止（SIGINT → SIGTERM）

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use regex::Regex;
use tauri::{AppHandle, Emitter, Manager};

/// 启动错误
#[derive(Debug, Clone, serde::Serialize)]
pub enum LaunchError {
    JavaNotFound,
    JarNotFound,
    LaunchFailed(String),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchError::JavaNotFound => write!(f, "找不到 Java 运行时"),
            LaunchError::JarNotFound => write!(f, "找不到 fnote.jar"),
            LaunchError::LaunchFailed(msg) => write!(f, "Java 进程启动失败：{}", msg),
        }
    }
}

impl std::error::Error for LaunchError {}

/// Java 子进程管理器
pub struct JavaServer {
    child: Arc<Mutex<Option<Child>>>,
    intentional_stop: Arc<Mutex<bool>>,
}

impl JavaServer {
    pub fn new() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            intentional_stop: Arc::new(Mutex::new(false)),
        }
    }

    /// 定位 java：打包后用资源目录 jre/bin/java，开发态用 JAVA_HOME / java_home
    pub fn locate_java(resource_dir: Option<&PathBuf>) -> Option<PathBuf> {
        // 打包态：App 资源目录内的 jre
        if let Some(res) = resource_dir {
            let bundled = res.join("jre").join("bin").join(java_exe_name());
            if bundled.exists() {
                return Some(bundled);
            }
        }
        // 开发态：JAVA_HOME
        if let Ok(home) = std::env::var("JAVA_HOME") {
            let p = PathBuf::from(home).join("bin").join(java_exe_name());
            if p.exists() {
                return Some(p);
            }
        }
        // macOS：/usr/libexec/java_home
        #[cfg(target_os = "macos")]
        {
            if let Ok(out) = std::process::Command::new("/usr/libexec/java_home").output() {
                if let Ok(home) = String::from_utf8(out.stdout) {
                    let home = home.trim();
                    if !home.is_empty() {
                        let p = PathBuf::from(home).join("bin").join(java_exe_name());
                        if p.exists() {
                            return Some(p);
                        }
                    }
                }
            }
        }
        // Windows：where java
        if let Ok(out) = std::process::Command::new("where").arg("java").output() {
            if out.status.success() {
                if let Ok(s) = String::from_utf8(out.stdout) {
                    if let Some(line) = s.lines().next() {
                        let p = PathBuf::from(line.trim());
                        if p.exists() {
                            return Some(p);
                        }
                    }
                }
            }
        }
        None
    }

    /// 定位 jar：打包后用资源目录 fnote.jar，开发态用 ../target 下的 maven 产物
    pub fn locate_jar(resource_dir: Option<&PathBuf>) -> Option<PathBuf> {
        if let Some(res) = resource_dir {
            let bundled = res.join("fnote.jar");
            if bundled.exists() {
                return Some(bundled);
            }
        }
        // 开发态：src-tauri/fnote.jar（build.sh 会复制过来）
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let local = manifest_dir.join("fnote.jar");
        if local.exists() {
            return Some(local);
        }
        // 开发态：项目根 target/ 下找 jar
        // src-tauri -> fnote-shell -> fnote-web -> target
        let dev_dir = manifest_dir
            .parent() // fnote-shell
            .and_then(|p| p.parent()) // fnote-web
            .map(|p| p.join("target"))?;
        if let Ok(entries) = std::fs::read_dir(&dev_dir) {
            let mut jars: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().map(|e| e == "jar").unwrap_or(false)
                        && !p
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.starts_with("original-"))
                            .unwrap_or(false)
                })
                .collect();
            jars.sort();
            return jars.into_iter().next();
        }
        None
    }

    /// 启动 Java 子进程，日志通过 emit 推送，就绪时 emit ready(port)
    pub fn start(
        &self,
        app: AppHandle,
        resource_dir: Option<PathBuf>,
    ) -> Result<(), LaunchError> {
        let java = Self::locate_java(resource_dir.as_ref()).ok_or(LaunchError::JavaNotFound)?;
        let jar = Self::locate_jar(resource_dir.as_ref()).ok_or(LaunchError::JarNotFound)?;

        // 上次若是异常退出（如 Dock 右键退出、强制退出），系统会直接终止 App，
        // 子进程来不及清理。启动时先杀掉指向同一个 jar 的残留服务进程。
        #[cfg(unix)]
        {
            let killed = Command::new("pkill")
                .arg("-f")
                .arg(jar.to_string_lossy().as_ref())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if killed {
                thread::sleep(Duration::from_millis(300));
            }
        }

        let mut cmd = Command::new(&java);
        cmd.arg("-jar")
            .arg(&jar)
            .arg("--server.port=0")
            .env("FNOTE_EMBEDDED", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| LaunchError::LaunchFailed(e.to_string()))?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        *self.child.lock().unwrap() = Some(child);
        *self.intentional_stop.lock().unwrap() = false;

        let child_arc = self.child.clone();
        let intentional = self.intentional_stop.clone();

        // 合并 stdout/stderr 逐行读取
        let app_out = app.clone();
        let app_err = app.clone();
        let app_exit = app.clone();

        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                handle_line(&app_out, &line);
            }
        });
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                handle_line(&app_err, &line);
            }
        });

        // 等待进程退出：轮询 try_wait，不长期持有 child 锁，
        // 否则 stop_and_wait 取锁会被阻塞，关闭窗口时界面卡死
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(200));
            let mut guard = match child_arc.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            match guard.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        let code = status.code();
                        guard.take();
                        drop(guard);
                        if !*intentional.lock().unwrap() {
                            let _ = app_exit.emit(
                                "server-exit",
                                format!(
                                    "本地服务已退出（退出码 {}）",
                                    code.map_or("unknown".to_string(), |c| c.to_string())
                                ),
                            );
                        }
                        return;
                    }
                    Ok(None) => {}
                    Err(_) => return,
                },
                // child 已被 stop_and_wait 接管
                None => return,
            }
        });

        Ok(())
    }

    /// 是否有正在运行的子进程
    pub fn is_running(&self) -> bool {
        match self.child.lock() {
            Ok(guard) => guard.is_some(),
            Err(_) => false,
        }
    }

    /// 优雅停止：先 SIGINT，超时再 SIGTERM
    pub fn stop_and_wait(&self, timeout_secs: u64) {
        *self.intentional_stop.lock().unwrap() = true;
        let mut child_opt = self.child.lock().unwrap().take();
        if let Some(child) = child_opt.as_mut() {
            // 先发中断信号让 Spring 优雅关停
            #[cfg(unix)]
            {
                // 直接用 kill 发 SIGINT 让 Spring 优雅关停
                unsafe {
                    libc_kill(child.id() as i32, libc_sigint());
                }
            }
            #[cfg(windows)]
            {
                // Windows 没有 SIGINT，用 terminate（taskkill /T）
                let _ = child.kill();
            }
            let deadline = Instant::now() + Duration::from_secs(timeout_secs);
            while Instant::now() < deadline {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
            // 超时强杀
            let _ = child.kill();
            let hard = Instant::now() + Duration::from_secs(3);
            while Instant::now() < hard {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

impl Default for JavaServer {
    fn default() -> Self {
        Self::new()
    }
}

// 跨平台 java 可执行文件名
fn java_exe_name() -> &'static str {
    #[cfg(windows)]
    {
        "java.exe"
    }
    #[cfg(not(windows))]
    {
        "java"
    }
}

// 端口解析与就绪探测
fn handle_line(app: &AppHandle, line: &str) {
    // 推送日志到前端
    let _ = app.emit("server-log", line.to_string());

    // 解析端口：Tomcat started on port(s): 53012 (http)
    let re = Regex::new(r"[Tt]omcat started on port.*?(\d{2,5})").unwrap();
    if let Some(caps) = re.captures(line) {
        if let Some(m) = caps.get(1) {
            if let Ok(port) = m.as_str().parse::<u16>() {
                // 记录端口到全局（用 app.manage 状态）
                if let Some(state) = app.try_state::<ServerState>() {
                    *state.port.lock().unwrap() = Some(port);
                }
            }
        }
    }

    // Spring Boot 启动完成标志
    if line.contains("Started Fnote2Application") {
        if let Some(state) = app.try_state::<ServerState>() {
            *state.started.lock().unwrap() = true;
        }
    }

    // 缓存最近日志，供日志窗口打开时回放
    if let Some(state) = app.try_state::<ServerState>() {
        let mut buf = state.logs.lock().unwrap();
        buf.push(line.to_string());
        if buf.len() > 2000 {
            buf.remove(0);
        }
    }

    // 端口 + started 都就绪 → emit ready
    if let Some(state) = app.try_state::<ServerState>() {
        let port = *state.port.lock().unwrap();
        let started = *state.started.lock().unwrap();
        let fired = *state.ready_fired.lock().unwrap();
        if !fired {
            if let (Some(port), true) = (port, started) {
                *state.ready_fired.lock().unwrap() = true;
                let _ = app.emit("server-ready", port);
            }
        }
    }
}

// 全局服务状态（端口、是否 started、是否已触发 ready）
#[derive(Default)]
pub struct ServerState {
    pub port: Mutex<Option<u16>>,
    pub started: Mutex<bool>,
    pub ready_fired: Mutex<bool>,
    pub logs: Mutex<Vec<String>>,
}

// Unix 信号辅助（避免引入 libc 依赖，用内联汇编或 nix 太重，这里用简单方式）
#[cfg(unix)]
unsafe fn libc_kill(pid: i32, sig: i32) {
    // 使用 libc crate 会更规范，但为减少依赖，直接调用系统
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    kill(pid, sig);
}

#[cfg(unix)]
fn libc_sigint() -> i32 {
    2 // SIGINT
}
