#!/bin/bash
# build.sh —— 一键构建 note&back.app（Spring Boot jar + jlink 内嵌 JRE + Tauri 壳）
#
# 环境要求：JDK 17+（含 jmods）、Maven、Rust、Node.js
# 用法：
#   ./build.sh            完整构建（先 mvn package）
#   SKIP_MAVEN=1 ./build.sh  跳过 maven，直接用 target/ 下现有 jar
#   OPEN=1 ./build.sh     构建后启动 App
#
# 构建产物统一放 $BUILD（默认 $HOME/build，可用 BUILD_DIR 覆盖）
set -euo pipefail
cd "$(dirname "$0")"
SHELL_DIR="$(pwd)"
FNOTE_WEB_DIR="${FNOTE_WEB_DIR:-$SHELL_DIR/../fnote-web}"
PROJECT_DIR="$(cd "$FNOTE_WEB_DIR" && pwd)"
BACKTOOL_DIR="${BACKTOOL_DIR:-$SHELL_DIR/../notebak-j/back-tool}"
BUILD="${BUILD_DIR:-$HOME/build}"
mkdir -p "$BUILD/fnote-shell" "$BUILD/fnote-web-target"

APP_NAME="note&back"

echo "==> [1/5] Maven 打包 Spring Boot fat jar"
if [ "${SKIP_MAVEN:-0}" != "1" ]; then
  (cd "$PROJECT_DIR" && mvn package -DskipTests -q)
else
  echo "    SKIP_MAVEN=1，跳过"
fi
JAR="$(ls "$PROJECT_DIR"/target/fnote-web-*.jar | grep -v '/original-' | head -1)"
[ -f "$JAR" ] || { echo "未找到 jar，请先执行 mvn package"; exit 1; }
# 解析为真实路径（target 可能是符号链接），tauri-build 不支持符号链接 resources
JAR="$(readlink -f "$JAR" || realpath "$JAR")"
echo "    使用 jar：$JAR"

# 组装前杀掉正在运行的旧实例（Tauri 壳 + Java 子进程），避免覆盖失败
pkill -f "note&back.app/Contents/MacOS" 2>/dev/null && sleep 1 || true
pkill -f "fnote.jar" 2>/dev/null && sleep 1 || true
pkill -f "BackTool.app/Contents/MacOS/BackTool" 2>/dev/null && sleep 1 || true

echo "==> [2/5] 准备资源（jar + jre，符号链接到 $BUILD 节省磁盘）"
# jar：符号链接到 src-tauri/fnote.jar
ln -sf "$JAR" "$SHELL_DIR/src-tauri/fnote.jar"
# jre：jlink 裁剪后放 $BUILD，符号链接到 src-tauri/jre
JAVA_HOME="$(/usr/libexec/java_home)"
JRE="$BUILD/fnote-shell/jre"
if [ ! -d "$JRE" ] || [ "${FORCE_JRE:-0}" = "1" ]; then
  echo "    jlink 裁剪内嵌 JRE（$("$JAVA_HOME/bin/java" -version 2>&1 | head -1)）"
  rm -rf "$JRE"
  "$JAVA_HOME/bin/jlink" \
    --module-path "$JAVA_HOME/jmods" \
    --add-modules \
java.base,java.compiler,java.logging,java.sql,java.xml,java.naming,java.desktop,\
java.management,java.instrument,java.security.jgss,java.security.sasl,java.net.http,\
java.transaction.xa,jdk.unsupported,jdk.crypto.ec,jdk.zipfs,jdk.localedata,jdk.management \
    --strip-debug --no-header-files --no-man-pages --compress=zip-6 \
    --output "$JRE"
else
  echo "    复用已有 JRE：$JRE"
fi
# jlink 生成的 legal 文件为只读且带 com.apple.provenance 属性，
# tauri-build 遍历时会 Permission denied。legal 仅为许可证文本，运行时不需要，直接删除。
rm -rf "$JRE/legal"
# jlink 生成的部分文件权限过严，统一放宽以便 tauri-build 遍历
chmod -R 755 "$JRE"
ln -sfn "$JRE" "$SHELL_DIR/src-tauri/jre"

echo "==> [3/5] 打包 BackTool（调用 back-tool/build-app.sh）"
if [ ! -x "$BACKTOOL_DIR/build-app.sh" ]; then
  echo "    未找到 $BACKTOOL_DIR/build-app.sh（可用 BACKTOOL_DIR 指定）" >&2
  exit 1
fi
# 产物放内置盘，不落代码盘；与 jre 一样用符号链接挂进 src-tauri
BT_OUT="${BT_OUT:-$BUILD/backtool-dist}"
if [ "${SKIP_BACKTOOL:-0}" != "1" ]; then
  OUT_DIR="$BT_OUT" "$BACKTOOL_DIR/build-app.sh"
else
  echo "    SKIP_BACKTOOL=1，跳过"
fi
BT_APP="$BT_OUT/BackTool.app"
[ -x "$BT_APP/Contents/MacOS/BackTool" ] || { echo "未找到 ${BT_APP}，请先执行 back-tool/build-app.sh" >&2; exit 1; }
rm -rf "$SHELL_DIR/src-tauri/back-tool"
ln -sfn "$BT_OUT" "$SHELL_DIR/src-tauri/back-tool"

echo "==> [4/5] 生成图标并 Tauri 构建"
cd "$SHELL_DIR"
# icon.png 不入库，打包时由 gen_icon.py 生成（依赖 Pillow，可用 SKIP_ICON=1 跳过）
if [ "${SKIP_ICON:-0}" != "1" ]; then
  python3 -c "import PIL" 2>/dev/null || python3 -m pip install --quiet --user Pillow
  (cd "$SHELL_DIR/src-tauri" && python3 gen_icon.py)
else
  echo "    SKIP_ICON=1，跳过"
fi
# target-dir 由 src-tauri/.cargo/config.toml 指定（与 IDE 共用同一路径，git 忽略）
npx @tauri-apps/cli build

echo "==> [5/5] ad-hoc 签名（本机运行；对外分发需 Developer ID）"
APP="$BUILD/fnote-shell/target/release/bundle/macos/$APP_NAME.app"
[ -d "$APP" ] || { echo "未找到构建产物：$APP"; exit 1; }
codesign --force --deep --sign - "$APP"

SIZE="$(du -sh "$APP" | awk '{print $1}')"
echo ""
echo "构建完成：${APP}（${SIZE}）"

# 部署到 /Applications
DEPLOY_TARGET="/Applications/$APP_NAME.app"
DO_DEPLOY="${DO_DEPLOY:-1}"
if [ "$DO_DEPLOY" = "1" ]; then
  echo ""
  echo "==> 部署到 $DEPLOY_TARGET"
  pkill -f "$APP_NAME.app/Contents/MacOS" 2>/dev/null || true
  pkill -f "fnote.jar" 2>/dev/null || true
  pkill -f "BackTool.app/Contents/MacOS/BackTool" 2>/dev/null || true
  sleep 1
  rm -rf "$DEPLOY_TARGET"
  cp -R "$APP" "$DEPLOY_TARGET"
  # 验证部署结果
  if [ -x "$DEPLOY_TARGET/Contents/MacOS/fnote-shell" ] && \
     [ -f "$DEPLOY_TARGET/Contents/Resources/fnote.jar" ] && \
     [ -x "$DEPLOY_TARGET/Contents/Resources/jre/bin/java" ] && \
     [ -x "$DEPLOY_TARGET/Contents/Resources/back-tool/BackTool.app/Contents/MacOS/BackTool" ]; then
    DEPLOY_SIZE="$(du -sh "$DEPLOY_TARGET" | awk '{print $1}')"
    echo "部署完成：${DEPLOY_TARGET}（${DEPLOY_SIZE}）"
  else
    echo "部署失败：产物不完整"
    exit 1
  fi
fi

if [ "${OPEN:-0}" = "1" ]; then
  open "$DEPLOY_TARGET"
fi
