// backtool.rs —— 内嵌 BackTool 子进程：整包随 App 分发，点首页卡片时拉起
// BackTool 自带 runtime，与 note&back 的 JRE 完全无关，只共享一次安装。

use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

static CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Child>> {
    CHILD.get_or_init(|| Mutex::new(None))
}

/// 内置 BackTool 可执行文件：打包态在资源目录，开发态用 build.sh 生成的那份
fn exe_path(resource_dir: Option<&PathBuf>) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Some(res) = resource_dir {
            let p = res
                .join("back-tool")
                .join("BackTool.app")
                .join("Contents")
                .join("MacOS")
                .join("BackTool");
            if p.exists() {
                return Some(p);
            }
        }
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("back-tool")
            .join("BackTool.app")
            .join("Contents")
            .join("MacOS")
            .join("BackTool");
        if dev.exists() {
            return Some(dev);
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = resource_dir;
        None
    }
}

/// 是否在运行；顺手回收已退出的子进程
pub fn is_running() -> bool {
    let mut guard = match slot().lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    let exited = match guard.as_mut() {
        Some(child) => match child.try_wait() {
            Ok(Some(_)) => true,
            Ok(None) => return true,
            Err(_) => return false,
        },
        None => return false,
    };
    if exited {
        guard.take();
    }
    false
}

/// 拉起 BackTool；已在运行则直接返回提示
pub fn start(app: AppHandle) -> Result<(), String> {
    if is_running() {
        return Err("BackTool 已在运行".to_string());
    }
    let resource_dir = app.path().resource_dir().ok();
    let exe = exe_path(resource_dir.as_ref())
        .ok_or_else(|| "找不到内置 BackTool.app，请先执行 ./build.sh".to_string())?;

    let mut cmd = Command::new(&exe);
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    let child = cmd
        .spawn()
        .map_err(|e| format!("启动 BackTool 失败：{}", e))?;

    match slot().lock() {
        Ok(mut guard) => *guard = Some(child),
        Err(_) => {
            let mut child = child;
            let _ = child.kill();
            return Err("BackTool 进程状态锁异常".to_string());
        }
    }
    watch(app);
    Ok(())
}

/// 监听子进程退出，通知前端恢复卡片状态
fn watch(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(200));
        let mut guard = match slot().lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let exited = match guard.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(_) => return,
            },
            None => return,
        };
        if exited {
            guard.take();
            drop(guard);
            let _ = app.emit("backtool-exit", ());
            return;
        }
    });
}

/// 关闭 BackTool：先 SIGTERM，最多等 grace_secs 秒，再强杀
pub fn stop(grace_secs: u64) {
    let child = slot().lock().ok().and_then(|mut g| g.take());
    let Some(mut child) = child else {
        return;
    };

    #[cfg(unix)]
    unsafe {
        send_signal(child.id() as i32, 15); // SIGTERM
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }

    let deadline = Instant::now() + Duration::from_secs(grace_secs);
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
unsafe fn send_signal(pid: i32, sig: i32) {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    kill(pid, sig);
}
