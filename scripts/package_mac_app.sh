#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT/apple/PlayerApp"
DIST_DIR="${DIST_DIR:-$ROOT/dist}"
BUNDLE_NAME="${BUNDLE_NAME:-Silent.app}"
FINAL_BUNDLE="$DIST_DIR/$BUNDLE_NAME"
ZIP_PATH="${ZIP_PATH:-$DIST_DIR/Silent-macos.zip}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
RELEASE_DIR="$TARGET_DIR/release"
APP_ICON="$APP_DIR/Resources/Silent.icns"
RUST_DYLIB="$RELEASE_DIR/libplayer_ffi.dylib"
ANALYZER_EXECUTABLE="$RELEASE_DIR/player_analyzer"
LIBRARY_WORKER_EXECUTABLE="$RELEASE_DIR/player_library_worker"
SIGN_IDENTITY="${CODESIGN_IDENTITY:--}"
STAGING_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/normalplayer-package.XXXXXX")"
BUNDLE="$STAGING_ROOT/$BUNDLE_NAME"

cleanup() {
    rm -rf "$STAGING_ROOT"
}
trap cleanup EXIT

if [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck source=/dev/null
    . "$HOME/.cargo/env"
fi

cd "$ROOT"
cargo build --release -p player_ffi -p player_analyzer -p player_library_worker
install_name_tool -id "@rpath/libplayer_ffi.dylib" "$RUST_DYLIB"

cd "$APP_DIR"
rm -f "$APP_DIR/.build/release/Silent"
swift build -c release --product Silent

EXECUTABLE="$APP_DIR/.build/release/Silent"
if [[ ! -x "$EXECUTABLE" ]]; then
    EXECUTABLE="$(find "$APP_DIR/.build" -path "*/release/Silent" -type f -perm -111 | head -n 1)"
fi
if [[ -z "${EXECUTABLE:-}" || ! -x "$EXECUTABLE" ]]; then
    echo "Silent release executable not found" >&2
    exit 1
fi
if ! otool -L "$EXECUTABLE" | grep -q '@rpath/libplayer_ffi.dylib'; then
    echo "Silent does not link player_ffi through @rpath" >&2
    exit 1
fi

mkdir -p "$DIST_DIR"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"
cp -X "$EXECUTABLE" "$BUNDLE/Contents/MacOS/Silent"
cp -X "$APP_ICON" "$BUNDLE/Contents/Resources/Silent.icns"
cp -X "$RUST_DYLIB" "$BUNDLE/Contents/MacOS/libplayer_ffi.dylib"
cp -X "$ANALYZER_EXECUTABLE" "$BUNDLE/Contents/MacOS/player_analyzer"
cp -X "$LIBRARY_WORKER_EXECUTABLE" "$BUNDLE/Contents/MacOS/player_library_worker"

cat > "$BUNDLE/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>Silent</string>
  <key>CFBundleIdentifier</key>
  <string>local.normalplayer.mac</string>
  <key>CFBundleDisplayName</key>
  <string>Silent</string>
  <key>CFBundleIconFile</key>
  <string>Silent.icns</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Silent</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.music</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key>
  <true/>
</dict>
</plist>
PLIST

plutil -lint "$BUNDLE/Contents/Info.plist"

BUNDLE_REAL="$(realpath "$BUNDLE")"
for item in \
    "$BUNDLE_REAL" \
    "$BUNDLE_REAL/Contents/MacOS/Silent" \
    "$BUNDLE_REAL/Contents/MacOS/libplayer_ffi.dylib" \
    "$BUNDLE_REAL/Contents/MacOS/player_analyzer" \
    "$BUNDLE_REAL/Contents/MacOS/player_library_worker"
do
    xattr -cr "$item" 2>/dev/null || true
    xattr -d com.apple.FinderInfo "$item" 2>/dev/null || true
    xattr -d "com.apple.fileprovider.fpfs#P" "$item" 2>/dev/null || true
    xattr -d com.apple.provenance "$item" 2>/dev/null || true
done

codesign --force --sign "$SIGN_IDENTITY" "$BUNDLE_REAL/Contents/MacOS/libplayer_ffi.dylib"
codesign --force --sign "$SIGN_IDENTITY" "$BUNDLE_REAL/Contents/MacOS/player_analyzer"
codesign --force --sign "$SIGN_IDENTITY" "$BUNDLE_REAL/Contents/MacOS/player_library_worker"
codesign --force --sign "$SIGN_IDENTITY" "$BUNDLE_REAL/Contents/MacOS/Silent"

for item in \
    "$BUNDLE_REAL/Contents/MacOS/Silent" \
    "$BUNDLE_REAL/Contents/MacOS/libplayer_ffi.dylib" \
    "$BUNDLE_REAL/Contents/MacOS/player_analyzer" \
    "$BUNDLE_REAL/Contents/MacOS/player_library_worker"
do
    xattr -d com.apple.FinderInfo "$item" 2>/dev/null || true
    xattr -d "com.apple.fileprovider.fpfs#P" "$item" 2>/dev/null || true
done

codesign --force --sign "$SIGN_IDENTITY" "$BUNDLE_REAL"
codesign --verify --deep --strict "$BUNDLE_REAL"

rm -f "$ZIP_PATH"
(cd "$STAGING_ROOT" && ditto -c -k --norsrc --noextattr --keepParent "$BUNDLE_NAME" "$ZIP_PATH")

VERIFY_ROOT="$STAGING_ROOT/verify"
mkdir -p "$VERIFY_ROOT"
ditto -x -k "$ZIP_PATH" "$VERIFY_ROOT"
codesign --verify --deep --strict "$VERIFY_ROOT/$BUNDLE_NAME"

rm -rf "$FINAL_BUNDLE"
ditto --norsrc --noextattr "$BUNDLE_REAL" "$FINAL_BUNDLE"

FINAL_BUNDLE_REAL="$(realpath "$FINAL_BUNDLE")"
xattr -d com.apple.FinderInfo "$FINAL_BUNDLE_REAL" 2>/dev/null || true
xattr -d "com.apple.fileprovider.fpfs#P" "$FINAL_BUNDLE_REAL" 2>/dev/null || true
if command -v SetFile >/dev/null 2>&1; then
    SetFile -a b "$FINAL_BUNDLE_REAL" 2>/dev/null || true
fi

echo "Packaged $FINAL_BUNDLE_REAL"
echo "Archive  $ZIP_PATH"
