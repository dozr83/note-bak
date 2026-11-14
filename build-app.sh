#!/bin/bash
# build-app.sh —— 打包 BackTool.app（独立 app-image，自带 JDK 17 runtime）
# 供 Fnote 等外层壳调用；产物默认放 /Users/gw/build，不落代码盘。
#
# 用法：
#   ./build-app.sh                     正常打包
#   SKIP_MAVEN=1 ./build-app.sh        跳过 mvn，复用 target/ 下现有产物
#   OUT_DIR=/xxx ./build-app.sh        指定产物目录
set -euo pipefail
cd "$(dirname "$0")"

APP_NAME="BackTool"
MAIN_CLASS="backtool.ui.main.BackToolMain"
OUT_DIR="${OUT_DIR:-/Users/gw/build/backtool-dist}"
STAGE_DIR="${STAGE_DIR:-/Users/gw/build/backtool-stage}"
TARGET_DIR="${TARGET_DIR:-/Users/gw/build/backtool-target}"
JDK17="${JDK17:-$(/usr/libexec/java_home -v 17 2>/dev/null || true)}"

if [ -z "$JDK17" ] || [ ! -x "$JDK17/bin/jpackage" ]; then
  echo "[back-tool] 未找到带 jpackage 的 JDK 17" >&2
  exit 1
fi

# 编译产物放内置盘，不落代码盘（target 为符号链接，与 fnote-web 的做法一致）
mkdir -p "$TARGET_DIR"
if [ ! -L target ]; then
  rm -rf target
  ln -sfn "$TARGET_DIR" target
fi

# !dmg：屏蔽 pom 里默认激活的 jpackage DMG，只出 jar + lib
if [ "${SKIP_MAVEN:-0}" != "1" ]; then
  JAVA_HOME="$JDK17" mvn -q package -DskipTests -P '!dmg'
fi
JAR="$(ls target/back-tool-*.jar 2>/dev/null | grep -v '/original-' | head -1)"
if [ ! -f "$JAR" ]; then
  echo "[back-tool] 未找到 jar，请检查 mvn package 输出" >&2
  exit 1
fi

# 只把 jar + 依赖打进 app-image，避免 target 下的编译中间产物撑大体积
rm -rf "$STAGE_DIR" "$OUT_DIR"
mkdir -p "$STAGE_DIR" "$OUT_DIR"
cp "$JAR" "$STAGE_DIR/"
cp -R target/lib "$STAGE_DIR/lib"

"$JDK17/bin/jpackage" \
  --type app-image \
  --name "$APP_NAME" \
  --main-jar "$(basename "$JAR")" \
  --main-class "$MAIN_CLASS" \
  --input "$STAGE_DIR" \
  --dest "$OUT_DIR"

APP="$OUT_DIR/$APP_NAME.app"
if [ ! -x "$APP/Contents/MacOS/$APP_NAME" ]; then
  echo "[back-tool] 生成失败：$APP" >&2
  exit 1
fi
echo "[back-tool] 产物：${APP}（$(du -sh "$APP" | awk '{print $1}')）"
