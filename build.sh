#!/usr/bin/env bash
# Build CopyPaster.app from SwiftPM executable.
# Output: ./CopyPaster.app — drag to /Applications или запускай отсюда.
set -euo pipefail

cd "$(dirname "$0")"

swift build -c release

APP="CopyPaster.app"
BIN=".build/release/CopyPaster"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/CopyPaster"
cp Resources/Info.plist "$APP/Contents/Info.plist"

# Ad-hoc sign (no Apple Dev cert). Gatekeeper показывает диалог при первом запуске.
codesign --force --deep --sign - "$APP"

echo "built: $(pwd)/$APP"
