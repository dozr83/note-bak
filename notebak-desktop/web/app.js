// app.js —— 首页逻辑：后台启动 Java 服务，就绪后「笔记」卡片可用
(function () {
  "use strict";
  const { invoke, listen } = window.App;

  const notesCard = document.getElementById("card-notes");
  const notesDesc = document.getElementById("notes-desc");
  const backtoolCard = document.getElementById("card-backtool");
  const backtoolDesc = document.getElementById("backtool-desc");
  const logCard = document.getElementById("card-log");
  const errorBar = document.getElementById("error");
  const errorText = document.getElementById("error-text");
  const retryBtn = document.getElementById("retry-btn");

  function showError(text) {
    errorText.textContent = text;
    errorBar.style.display = "flex";
  }

  function hideError() {
    errorBar.style.display = "none";
  }

  async function start() {
    hideError();
    notesCard.disabled = true;
    notesDesc.textContent = "启动中…";
    try {
      await invoke("start_server");
    } catch (e) {
      notesDesc.textContent = "启动失败";
      showError(e);
    }
  }

  // 服务就绪：启用笔记卡片
  listen("server-ready", () => {
    notesCard.disabled = false;
    notesDesc.textContent = "note&back Web";
  });

  // 服务意外退出
  listen("server-exit", (e) => {
    notesCard.disabled = true;
    notesDesc.textContent = "服务已退出";
    showError(e.payload);
  });

  notesCard.addEventListener("click", async () => {
    try {
      await invoke("open_notes");
    } catch (e) {
      showError(e);
    }
  });

  logCard.addEventListener("click", () => invoke("open_log_window"));

  // 备份小工具：独立子进程，弹自己的窗口；关掉后卡片恢复可点
  backtoolCard.addEventListener("click", async () => {
    try {
      await invoke("open_backtool");
      backtoolDesc.textContent = "已打开";
    } catch (e) {
      const msg = String(e);
      if (msg.includes("已在运行")) {
        backtoolDesc.textContent = "已打开";
        return;
      }
      backtoolDesc.textContent = "打开失败";
      showError(msg);
    }
  });
  listen("backtool-exit", () => {
    backtoolDesc.textContent = "点击打开";
  });

  retryBtn.addEventListener("click", start);

  start();
})();
