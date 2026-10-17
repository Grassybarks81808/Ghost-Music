#!/usr/bin/env bash
# Installs a user-scoped JDK 21, Android SDK command-line tools, the Android 35
# platform, and Gradle 8.9 into ~/tools, then accepts the SDK licences.
set -euo pipefail

TOOLS="$HOME/tools"
mkdir -p "$TOOLS"
cd "$TOOLS"

if [ ! -x "$TOOLS/jdk21/bin/java.exe" ]; then
  echo "== downloading JDK 21"
  curl -fL --retry 3 -o jdk21.zip "https://api.adoptium.net/v3/binary/latest/21/ga/windows/x64/jdk/hotspot/normal/eclipse?project=jdk"
  echo "== extracting JDK"
  rm -rf jdk21-extract && mkdir jdk21-extract
  unzip -q jdk21.zip -d jdk21-extract
  JDK_DIR=$(find jdk21-extract -maxdepth 1 -mindepth 1 -type d | head -1)
  mv "$JDK_DIR" jdk21
  rm -rf jdk21-extract jdk21.zip
fi
export JAVA_HOME="$TOOLS/jdk21"
export PATH="$JAVA_HOME/bin:$PATH"
java -version

if [ ! -x "$TOOLS/android-sdk/cmdline-tools/latest/bin/sdkmanager.bat" ]; then
  echo "== downloading Android command-line tools"
  curl -fL --retry 3 -o cmdline-tools.zip "https://dl.google.com/android/repository/commandlinetools-win-13114758_latest.zip"
  rm -rf cmdline-tools-extract && mkdir -p cmdline-tools-extract android-sdk/cmdline-tools
  unzip -q cmdline-tools.zip -d cmdline-tools-extract
  mv cmdline-tools-extract/cmdline-tools android-sdk/cmdline-tools/latest
  rm -rf cmdline-tools-extract cmdline-tools.zip
fi

export ANDROID_HOME="$TOOLS/android-sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager.bat"

echo "== accepting SDK licences"
yes | "$SDKM" --licenses > /dev/null || true

echo "== installing platform-tools, android-35, build-tools 35.0.0"
"$SDKM" --install "platform-tools" "platforms;android-35" "build-tools;35.0.0"

if [ ! -x "$TOOLS/gradle-8.9/bin/gradle" ]; then
  echo "== downloading Gradle 8.9"
  curl -fL --retry 3 -o gradle.zip "https://services.gradle.org/distributions/gradle-8.9-bin.zip"
  unzip -q gradle.zip
  rm -f gradle.zip
fi

echo "== toolchain ready"
"$TOOLS/gradle-8.9/bin/gradle" --version | head -8
"$SDKM" --list_installed | head -20
