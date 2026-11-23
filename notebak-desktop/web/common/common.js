// common.js —— 所有页面共享的唯一全局出口（MPA 架构，每页只加载本文件 + 自己的页面脚本）。
// 页面脚本一律用 window.App.xxx，禁止自行声明全局变量；自身用 IIFE 包裹。
(function () {
  "use strict";

  function escapeHtml(s) {
    return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  // 注入确认弹窗 DOM（样式在 style.css，页面 HTML 不必重复写）
  function injectModal() {
    if (document.getElementById("modal-overlay")) return;
    const overlay = document.createElement("div");
    overlay.id = "modal-overlay";
    overlay.className = "modal-overlay";
    overlay.style.display = "none";
    overlay.innerHTML =
      '<div class="modal">' +
      '<div id="modal-title" class="modal-title"></div>' +
      '<div id="modal-message" class="modal-message"></div>' +
      '<div class="row" style="justify-content:flex-end">' +
      '<button id="modal-cancel">取消</button>' +
      '<button id="modal-ok" class="primary">确认</button>' +
      "</div></div>";
    document.body.appendChild(overlay);
  }

  function showConfirm(title, message) {
    return new Promise((resolve) => {
      injectModal();
      document.getElementById("modal-title").textContent = title;
      document.getElementById("modal-message").textContent = message;
      document.getElementById("modal-overlay").style.display = "flex";
      document.getElementById("modal-ok").onclick = () => {
        document.getElementById("modal-overlay").style.display = "none";
        resolve(true);
      };
      document.getElementById("modal-cancel").onclick = () => {
        document.getElementById("modal-overlay").style.display = "none";
        resolve(false);
      };
    });
  }

  // 通用 conf 解析：逐行 key=value，忽略空行/注释，值原样为字符串。
  function parseConfFlat(text) {
    const m = {};
    for (const line of String(text || "").split("\n")) {
      const s = line.trim();
      if (s === "" || s.startsWith("#") || !s.includes("=")) continue;
      const eq = s.indexOf("=");
      m[s.slice(0, eq).trim()] = s.slice(eq + 1).trim();
    }
    return m;
  }

  window.App = {
    invoke: window.__TAURI__.core.invoke,
    listen: window.__TAURI__.event ? window.__TAURI__.event.listen : null,
    escapeHtml,
    showConfirm,
    parseConfFlat,
  };
})();
