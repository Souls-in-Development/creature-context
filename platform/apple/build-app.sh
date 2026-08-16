#!/usr/bin/env bash
# Build AtlasMenuBar as a proper dock-free .app bundle.
#
# Why a bundle: a bare `swift run AtlasMenuBar` starts an un-bundled process,
# and macOS does not reliably give such a process a menu-bar status item. A real
# .app (with Info.plist LSUIElement) does — and you can double-click it or add it
# to Login Items. This script builds the binary and assembles the bundle.
set -euo pipefail
cd "$(dirname "$0")"

CONFIG="${1:-release}"
swift build -c "$CONFIG" --product AtlasMenuBar
BIN="$(swift build -c "$CONFIG" --product AtlasMenuBar --show-bin-path)/AtlasMenuBar"

APP="AtlasMenuBar.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$BIN" "$APP/Contents/MacOS/AtlasMenuBar"
cp AtlasMenuBar-Info.plist "$APP/Contents/Info.plist"

echo "Built: $(pwd)/$APP"
echo "Launch:  open '$(pwd)/$APP'    (look for the map icon in the menu bar)"
