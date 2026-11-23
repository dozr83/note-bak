# notebak-desktop —— NoteBak 桌面端（Tauri + 内嵌 JRE）

Tauri 桌面壳，卡片式首页，启动时拉起内嵌服务与工具：
- **笔记**：内部跑 `fnote.jar`，壳内 WebView 打开 fnote-web 页面
- **备份小工具**：拉起内置 `BackTool.app`（来自 `../notebak-j/back-tool`）
- **脚本**：脚本执行器（读 script.conf，跑 python3）
- **服务日志**：独立窗口显示 Java 服务日志

## 目录结构

```
notebak-desktop/
├── build.sh              一键构建脚本
├── web/                  前端静态页面（卡片首页 + 日志窗口）
│   ├── index.html        首页功能卡片
│   ├── common/           公共样式与 window.App（MPA 共享出口）
│   ├── pyexec/           脚本执行器页面
│   └── log.html          服务日志独立窗口
└── src-tauri/            Tauri Rust 工程
    ├── src/
    │   ├── main.rs       二进制入口
    │   ├── lib.rs        Tauri 命令、菜单、窗口事件
    │   ├── java_server.rs Java 子进程生命周期
    │   ├── backtool.rs   内嵌 BackTool 子进程
    │   └── pyexec.rs     脚本执行器后端
    ├── capabilities/default.json  权限声明
    └── tauri.conf.json   Tauri 配置
```

## 工作原理

```
note&back.app
└── Contents/
    ├── MacOS/fnote-shell     Tauri 壳
    └── Resources/
        ├── fnote.jar         Spring Boot fat jar（来自 fnote-web）
        ├── jre/              jlink 裁剪的内嵌 JRE
        └── back-tool/        BackTool.app 整包（来自 ../notebak-j/back-tool）
```

1. 壳启动 → 定位 App 内 `jre/bin/java`、`fnote.jar`、`back-tool/BackTool.app`
2. 以 `--server.port=0` 启动 fnote.jar，就绪后解析端口
3. 点「笔记」→ 新窗口打开 `http://localhost:<端口>/`
4. 点「备份小工具」→ 拉起内置 BackTool.app（自带 runtime，与 JRE 无关）
5. 退出 App → 优雅停 Java 与 BackTool，不残留进程

## 外部依赖

- **fnote-web**：构建笔记功能需外部 fnote-web 产物，路径由 `FNOTE_WEB_DIR` 指定（默认 `../fnote-web`，后续将迁入 `notebak-j`）。仅改 Rust/前端时可用 `SKIP_MAVEN=1` 跳过 maven。
- **back-tool**：默认取 `../notebak-j/back-tool`，可用 `BACKTOOL_DIR` 覆盖。

## 环境变量：FNOTE_EMBEDDED

壳启动 Java 子进程时注入 `FNOTE_EMBEDDED=1`，fnote-web 据此不弹外部浏览器，页面由壳内 WebView 显示。

## 菜单栏

功能 → 笔记（⌘1）／备份小工具（⌘2）／首页（⌘0）／服务日志…（⌘L）

## 构建

环境要求：JDK 17+（含 jmods）、Maven、Rust、Node.js。

```bash
cd notebak-desktop
./build.sh                 # 完整构建：mvn + jlink + 打 BackTool + tauri build + 签名
SKIP_MAVEN=1 ./build.sh    # 跳过 maven
SKIP_BACKTOOL=1 ./build.sh # 跳过 BackTool 打包
OPEN=1 ./build.sh          # 构建后启动
FORCE_JRE=1 ./build.sh     # 强制重新生成 JRE
```

构建产物目录由 `BUILD_DIR` 指定（默认 `$HOME/build`）：
- App：`$BUILD_DIR/fnote-shell/target/release/bundle/macos/note&back.app`
- DMG：`$BUILD_DIR/fnote-shell/target/release/bundle/dmg/note&back_*.dmg`

BackTool 产物输出到 `$BUILD_DIR/backtool-dist`（可用 `BT_OUT` 覆盖）。

## 开发态

```bash
cd notebak-desktop
npx @tauri-apps/cli dev
```
