#!/usr/bin/env bash
# 构建 DawnGlow 的 Android APK。
#
# 不使用 Gradle：cargo 交叉编译出 .so + aapt2 生成 R.java + javac/d8 编译出 classes.dex，
# 再手工打包 .so / classes.dex，最后 zipalign + 签名。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-17-openjdk}"
export ANDROID_HOME="${ANDROID_HOME:-/root/android/sdk}"
export PATH="$JAVA_HOME/bin:$HOME/.cargo/bin:/usr/bin:$PATH"

BT="$ANDROID_HOME/build-tools/35.0.0"
PLATFORM="$ANDROID_HOME/platforms/android-36/android.jar"

TARGET="${TARGET:-aarch64-linux-android}"
ABI="arm64-v8a"
APP_NAME="DawnGlow"
VERSION_NAME="0.1.0"
VERSION_CODE="1"
PKG="top.izuna.dawnglow"

OUT="$ROOT/build"
GEN="$OUT/gen"
CLASSES="$OUT/classes"
DEX="$OUT/dex"
rm -rf "$GEN" "$CLASSES" "$DEX"
mkdir -p "$OUT" "$GEN" "$CLASSES" "$DEX"

echo "==> 1/6 编译 Rust cdylib ($TARGET)"
cd "$ROOT"
cargo build --release --lib --target "$TARGET"
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import sys,json;print(json.load(sys.stdin)["target_directory"])')"
SO="$TARGET_DIR/$TARGET/release/libdawnglow.so"
[ -f "$SO" ] || { echo "未找到 $SO"; exit 1; }
echo "    so: $SO ($(stat -c %s "$SO") bytes)"

echo "==> 2/6 aapt2 compile"
RESZIP="$OUT/res.zip"
"$BT/aapt2" compile --dir "$ROOT/android/res" -o "$RESZIP"

echo "==> 3/6 aapt2 link（并生成 R.java）"
RAW="$OUT/base.apk"
"$BT/aapt2" link -o "$RAW" \
  --manifest "$ROOT/AndroidManifest.xml" \
  -I "$PLATFORM" \
  "$RESZIP" \
  --java "$GEN" \
  --min-sdk-version 26 \
  --target-sdk-version 34 \
  --version-code "$VERSION_CODE" \
  --version-name "$VERSION_NAME" \
  --rename-manifest-package "$PKG"

echo "==> 4/6 javac + d8（小组件 Java 代码）"
mapfile -t JAVA_SRC < <(find "$ROOT/android/java" "$GEN" -name '*.java')
javac -encoding UTF-8 -source 8 -target 8 -Xlint:-options \
  -classpath "$PLATFORM" -d "$CLASSES" "${JAVA_SRC[@]}"
mapfile -t CLASS_FILES < <(find "$CLASSES" -name '*.class')
"$BT/d8" --release --min-api 26 --lib "$PLATFORM" \
  --output "$DEX" "${CLASS_FILES[@]}"
[ -f "$DEX/classes.dex" ] || { echo "未生成 classes.dex"; exit 1; }
echo "    classes.dex: $(stat -c %s "$DEX/classes.dex") bytes"

echo "==> 5/6 打包 .so + classes.dex"
python3 - "$RAW" "$SO" "$DEX/classes.dex" "lib/$ABI/libdawnglow.so" <<'PY'
import sys, zipfile
apk, so, dex, so_arc = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
with zipfile.ZipFile(apk, 'a', zipfile.ZIP_DEFLATED) as z:
    z.write(so, so_arc)
    z.write(dex, 'classes.dex')
PY

echo "==> 6/6 zipalign + 签名"
ALIGNED="$OUT/$APP_NAME-$VERSION_NAME-aligned.apk"
"$BT/zipalign" -f -p 4 "$RAW" "$ALIGNED"

KS="$OUT/debug.keystore"
if [ ! -f "$KS" ]; then
  keytool -genkeypair -keystore "$KS" -storepass android -keypass android \
    -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 \
    -dname "CN=Android Debug,O=Android,C=US"
fi

FINAL="$OUT/$APP_NAME-$VERSION_NAME-debug.apk"
"$BT/apksigner" sign \
  --ks "$KS" --ks-pass pass:android --key-pass pass:android \
  --ks-key-alias androiddebugkey \
  --out "$FINAL" "$ALIGNED"

echo
echo "APK 输出: $FINAL"
"$BT/apksigner" verify --print-certs "$FINAL" | head -3 || true
