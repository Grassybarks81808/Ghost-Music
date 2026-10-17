#!/usr/bin/env bash
# Builds a signed release APK with the locally installed JDK/Android SDK.
# Run scripts/install-android-toolchain.sh once first.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS="$HOME/tools"
export JAVA_HOME="${JAVA_HOME:-$TOOLS/jdk21}"
export ANDROID_HOME="${ANDROID_HOME:-$TOOLS/android-sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export PATH="$JAVA_HOME/bin:$PATH"
GRADLE="${GRADLE:-$TOOLS/gradle-8.9/bin/gradle}"

if [ ! -x "$JAVA_HOME/bin/java.exe" ] && [ ! -x "$JAVA_HOME/bin/java" ]; then
  echo "JDK not found at $JAVA_HOME. Run scripts/install-android-toolchain.sh first." >&2
  exit 1
fi
if [ ! -x "$GRADLE" ]; then
  echo "Gradle not found at $GRADLE. Run scripts/install-android-toolchain.sh first." >&2
  exit 1
fi

cd "$ROOT/android"

if [ ! -f keystore.properties ]; then
  echo "› creating a release keystore"
  keytool -genkeypair -v \
    -keystore ghost-release.jks \
    -alias ghost \
    -keyalg RSA -keysize 2048 -validity 10000 \
    -storepass ghostmusic -keypass ghostmusic \
    -dname "CN=Ghost Music, O=Ghost Music, C=US" > /dev/null
  cat > keystore.properties <<EOF
storeFile=ghost-release.jks
storePassword=ghostmusic
keyAlias=ghost
keyPassword=ghostmusic
EOF
fi

echo "› gradle assembleRelease"
"$GRADLE" --no-daemon --console=plain assembleRelease

mkdir -p "$ROOT/dist"
APK="$ROOT/android/app/build/outputs/apk/release/app-release.apk"
cp "$APK" "$ROOT/dist/GhostMusic-1.0.0.apk"
echo "✓ dist/GhostMusic-1.0.0.apk"
