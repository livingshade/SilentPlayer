#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if ! xcrun --sdk iphoneos --show-sdk-path >/dev/null 2>&1; then
  echo "error: iphoneos SDK not found. Install full Xcode and run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer" >&2
  exit 1
fi

if ! xcrun --sdk iphonesimulator --show-sdk-path >/dev/null 2>&1; then
  echo "error: iphonesimulator SDK not found. Install full Xcode and run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer" >&2
  exit 1
fi

rustup target add aarch64-apple-ios aarch64-apple-ios-sim

export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-16.0}"

cargo build -p player_ffi --release --target aarch64-apple-ios
cargo build -p player_ffi --release --target aarch64-apple-ios-sim

APP_ROOT="apple/PlayerApp"
XCFRAMEWORK="$APP_ROOT/Vendor/PlayerFFI.xcframework"

rm -rf "$XCFRAMEWORK"
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libplayer_ffi.a \
  -headers "$APP_ROOT/Sources/PlayerRustFFI/include" \
  -library target/aarch64-apple-ios-sim/release/libplayer_ffi.a \
  -headers "$APP_ROOT/Sources/PlayerRustFFI/include" \
  -output "$XCFRAMEWORK"

echo "Built:"
echo "  target/aarch64-apple-ios/release/libplayer_ffi.a"
echo "  target/aarch64-apple-ios-sim/release/libplayer_ffi.a"
echo "  $XCFRAMEWORK"
