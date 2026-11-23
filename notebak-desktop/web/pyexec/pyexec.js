// 脚本执行器页：配置文件路径存 localStorage，后端读该文件生成按钮
// 迁移自 PlayLang4mpv；默认路径留空，首次使用请点「选择」指定 script.conf
(function () {
  "use strict";
  const { invoke, listen } = window.App;

  const log = document.getElementById("pyexec-log");
  const buttonPane = document.getElementById("button-pane");
  const confInput = document.getElementById("pyexec-conf-input");
  const saveBtn = document.getElementById("pyexec-save-btn");
  const resetBtn = document.getElementById("pyexec-reset-btn");
  const browseBtn = document.getElementById("pyexec-browse-btn");
  const helpBtn = document.getElementById("pyexec-help-btn");

  const DEFAULT_CONF_PATH = "";

  const HELP_TEXT =
    "脚本执行器使用说明\n" +
    "\n" +
    "1. 配置文件路径：指向一个 .conf 文件，里面定义按钮和命令\n" +
    "2. 点击「选择」可从文件系统选配置文件\n" +
    "3. 点击「恢复默认」回到默认配置路径\n" +
    "4. 路径存 localStorage，下次打开自动加载\n" +
    "\n" +
    "配置文件格式（Properties 风格）：\n" +
    "  # 注释行\n" +
    "  script.序号.name=按钮显示名称\n" +
    "  script.序号.cmd=脚本文件名 参数...\n" +
    "\n" +
    "示例：\n" +
    "  script.1.name=备份\n" +
    "  script.1.cmd=md5_tool.py -g -d /path/to/your-dir --git\n" +
    "\n" +
    "说明：\n" +
    "- 脚本文件和配置文件放在同一目录\n" +
    "- cmd 里脚本名在前，参数在后（空格分隔）\n" +
    "- 通过 python3 stdin 方式执行脚本内容";

  function logLine(s) {
    log.textContent += s + "\n";
    log.scrollTop = log.scrollHeight;
  }

  // 监听后端 stdout/stderr 事件
  listen("script-output", (e) => {
    logLine(e.payload);
  });

  async function loadScripts(confPath) {
    try {
      const result = await invoke("pyexec_list", { confPath: confPath });
      buttonPane.innerHTML = "";
      const scripts = result.scripts;
      if (scripts.length === 0) {
        logLine("无脚本配置。请在 " + confPath + " 里添加 script.N.name / script.N.cmd");
        return;
      }
      log.textContent = "";
      scripts.forEach((item) => {
        const btn = document.createElement("button");
        btn.textContent = item.name;
        btn.style.minWidth = "120px";
        btn.onclick = async () => {
          log.textContent = "";
          logLine("-> 正在执行: " + item.cmd);
          try {
            await invoke("pyexec_run", { confPath: confPath, cmd: item.cmd });
          } catch (e) {
            logLine("启动失败：" + e);
          }
        };
        buttonPane.appendChild(btn);
      });
    } catch (e) {
      logLine("加载失败：" + e);
    }
  }

  // 保存并加载
  async function saveAndLoad() {
    const confPath = confInput.value.trim();
    if (!confPath) {
      logLine("请先填配置路径");
      return;
    }
    localStorage.setItem("script_conf_path", confPath);
    log.textContent = "";
    logLine("已保存，加载中...");
    await loadScripts(confPath);
  }

  // 保存按钮或按 Enter 都触发
  saveBtn.onclick = saveAndLoad;

  // 选择配置文件（走 Rust 侧系统对话框）
  browseBtn.onclick = async () => {
    const path = await invoke("pick_conf_file");
    if (!path) return;
    confInput.value = path;
    localStorage.setItem("script_conf_path", path);
    log.textContent = "";
    await loadScripts(path);
  };

  // 使用说明（用日志区展示，避免依赖系统 alert）
  helpBtn.onclick = () => {
    log.textContent = HELP_TEXT;
  };

  // 恢复默认路径
  resetBtn.onclick = async () => {
    confInput.value = DEFAULT_CONF_PATH;
    localStorage.setItem("script_conf_path", DEFAULT_CONF_PATH);
    log.textContent = "";
    await loadScripts(DEFAULT_CONF_PATH);
  };
  confInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      saveAndLoad();
    }
  });

  // 进入页面：读 localStorage，没有就用默认路径；非空才自动加载
  const savedPath = localStorage.getItem("script_conf_path") || DEFAULT_CONF_PATH;
  confInput.value = savedPath;
  if (savedPath) loadScripts(savedPath);
})();
