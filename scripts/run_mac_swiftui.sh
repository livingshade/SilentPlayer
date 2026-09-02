#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT/apple/PlayerApp"
BUNDLE="$APP_DIR/.build/debug/Silent.app"
EXECUTABLE="$APP_DIR/.build/debug/Silent"
APP_ICON="$APP_DIR/Resources/Silent.icns"
RUST_DYLIB="$ROOT/target/debug/libapp_ffi.dylib"
ANALYZER_EXECUTABLE="$ROOT/target/debug/analyzer"
LIBRARY_WORKER_EXECUTABLE="$ROOT/target/debug/library_worker"

. "$HOME/.cargo/env"
cd "$ROOT"
cargo build -p app_ffi -p analyzer -p library_worker
install_name_tool -id "@rpath/libapp_ffi.dylib" "$RUST_DYLIB"

cd "$APP_DIR"
rm -f "$EXECUTABLE"
swift build --product Silent

if ! otool -L "$EXECUTABLE" | grep -q '@rpath/libapp_ffi.dylib'; then
    echo "Silent does not link app_ffi through @rpath" >&2
    exit 1
fi

rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"
cp -X "$EXECUTABLE" "$BUNDLE/Contents/MacOS/Silent"
cp -X "$APP_ICON" "$BUNDLE/Contents/Resources/Silent.icns"
cp -X "$RUST_DYLIB" "$BUNDLE/Contents/MacOS/"
cp -X "$ANALYZER_EXECUTABLE" "$BUNDLE/Contents/MacOS/analyzer"
cp -X "$LIBRARY_WORKER_EXECUTABLE" "$BUNDLE/Contents/MacOS/library_worker"

cat > "$BUNDLE/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>Silent</string>
  <key>CFBundleIdentifier</key>
  <string>local.normalplayer.mac</string>
  <key>CFBundleDisplayName</key>
  <string>Silent</string>
  <key>CFBundleIconFile</key>
  <string>Silent.icns</string>
  <key>CFBundleName</key>
  <string>Silent</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>26.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

BUNDLE_REAL="$(realpath "$BUNDLE")"
xattr -cr "$BUNDLE_REAL"
xattr -d com.apple.FinderInfo "$BUNDLE_REAL" 2>/dev/null || true
xattr -d "com.apple.fileprovider.fpfs#P" "$BUNDLE_REAL" 2>/dev/null || true
xattr -cr "$BUNDLE_REAL/Contents/MacOS/libapp_ffi.dylib"
xattr -cr "$BUNDLE_REAL/Contents/MacOS/analyzer"
xattr -cr "$BUNDLE_REAL/Contents/MacOS/library_worker"
xattr -cr "$BUNDLE_REAL/Contents/MacOS/Silent"
xattr -d com.apple.provenance "$BUNDLE_REAL" 2>/dev/null || true
xattr -d com.apple.provenance "$BUNDLE_REAL/Contents/MacOS/libapp_ffi.dylib" 2>/dev/null || true
xattr -d com.apple.provenance "$BUNDLE_REAL/Contents/MacOS/analyzer" 2>/dev/null || true
xattr -d com.apple.provenance "$BUNDLE_REAL/Contents/MacOS/library_worker" 2>/dev/null || true
xattr -d com.apple.provenance "$BUNDLE_REAL/Contents/MacOS/Silent" 2>/dev/null || true
codesign --force --sign - "$BUNDLE_REAL/Contents/MacOS/libapp_ffi.dylib"
codesign --force --sign - "$BUNDLE_REAL/Contents/MacOS/analyzer"
codesign --force --sign - "$BUNDLE_REAL/Contents/MacOS/library_worker"
codesign --force --sign - "$BUNDLE_REAL/Contents/MacOS/Silent"
codesign --force --sign - "$BUNDLE_REAL"
codesign --verify --deep --strict "$BUNDLE_REAL"

if [[ "${SILENT_SKIP_OPEN:-0}" == "1" ]]; then
    echo "Built $BUNDLE_REAL"
else
    open -n "$BUNDLE_REAL"
    echo "Opened $BUNDLE_REAL"
fi
