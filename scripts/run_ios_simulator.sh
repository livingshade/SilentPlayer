#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

SIM_DEVICE_NAME="${NORMALPLAYER_SIM_DEVICE:-iPhone 17}"
SIM_BUNDLE_ID="${NORMALPLAYER_IOS_BUNDLE_ID:-com.normalplayer.ios}"
APP_ROOT="apple/PlayerApp"
SWIFT_SCRATCH="$APP_ROOT/.build-ios-sim"
PRODUCT_DIR="$SWIFT_SCRATCH/arm64-apple-ios-simulator/debug"
APP_BUNDLE="$PRODUCT_DIR/NormalPlayer-iOS.app"
LAUNCH_SCREEN_SOURCE="$APP_ROOT/Resources/LaunchScreen.storyboard"
FIXTURE_SOURCE="${NORMALPLAYER_IOS_FIXTURES:-test-assets/audio}"
FIXTURE_DEST_NAME="${NORMALPLAYER_IOS_FIXTURE_FOLDER:-ImportFixtures}"

find_sim_udid() {
  xcrun simctl list devices available \
    | sed -n "s/^[[:space:]]*${SIM_DEVICE_NAME} (\([0-9A-F-]*\)) .*/\1/p" \
    | head -n 1
}

require_sim_runtime() {
  if ! xcrun simctl list runtimes available | grep -q "com.apple.CoreSimulator.SimRuntime.iOS"; then
    echo "error: no iOS Simulator runtime is installed." >&2
    echo "Install it with: xcodebuild -downloadPlatform iOS" >&2
    exit 1
  fi
}

ensure_xcframework() {
  ./scripts/build_ios_rust.sh
}

build_swift_product() {
  (
    cd "$APP_ROOT"
    swift build \
      --triple arm64-apple-ios16.0-simulator \
      --sdk "$(xcrun --sdk iphonesimulator --show-sdk-path)" \
      --scratch-path ".build-ios-sim" \
      --product NormalPlayer-iOS
  )
}

write_info_plist() {
  local plist="$APP_BUNDLE/Info.plist"

  plutil -create xml1 "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleExecutable string NormalPlayer-iOS" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string ${SIM_BUNDLE_ID}" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleName string NormalPlayer" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleDisplayName string NormalPlayer" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundlePackageType string APPL" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleVersion string 1" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string 0.1" "$plist"
  /usr/libexec/PlistBuddy -c "Add :MinimumOSVersion string 16.0" "$plist"
  /usr/libexec/PlistBuddy -c "Add :LSRequiresIPhoneOS bool true" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UILaunchStoryboardName string LaunchScreen" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UIFileSharingEnabled bool true" "$plist"
  /usr/libexec/PlistBuddy -c "Add :LSSupportsOpeningDocumentsInPlace bool true" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UTExportedTypeDeclarations array" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UTExportedTypeDeclarations:0 dict" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UTExportedTypeDeclarations:0:UTTypeIdentifier string com.normalplayer.silent-library" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UTExportedTypeDeclarations:0:UTTypeDescription string Silent Library Package" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UTExportedTypeDeclarations:0:UTTypeConformsTo array" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UTExportedTypeDeclarations:0:UTTypeConformsTo:0 string com.apple.package" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UTExportedTypeDeclarations:0:UTTypeTagSpecification dict" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UTExportedTypeDeclarations:0:UTTypeTagSpecification:public.filename-extension array" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UTExportedTypeDeclarations:0:UTTypeTagSpecification:public.filename-extension:0 string silentlibrary" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleDocumentTypes array" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleDocumentTypes:0 dict" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleDocumentTypes:0:CFBundleTypeName string Silent Library Package" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleDocumentTypes:0:LSItemContentTypes array" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleDocumentTypes:0:LSItemContentTypes:0 string com.normalplayer.silent-library" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleDocumentTypes:0:LSHandlerRank string Owner" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UIBackgroundModes array" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UIBackgroundModes:0 string audio" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UIDeviceFamily array" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UIDeviceFamily:0 integer 1" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UIDeviceFamily:1 integer 2" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UISupportedInterfaceOrientations array" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UISupportedInterfaceOrientations:0 string UIInterfaceOrientationPortrait" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UISupportedInterfaceOrientations:1 string UIInterfaceOrientationLandscapeLeft" "$plist"
  /usr/libexec/PlistBuddy -c "Add :UISupportedInterfaceOrientations:2 string UIInterfaceOrientationLandscapeRight" "$plist"
}

package_app_bundle() {
  local binary="$PRODUCT_DIR/NormalPlayer-iOS"

  if [[ ! -x "$binary" ]]; then
    echo "error: expected iOS simulator binary at $binary" >&2
    exit 1
  fi

  rm -rf "$APP_BUNDLE"
  mkdir -p "$APP_BUNDLE"
  cp "$binary" "$APP_BUNDLE/NormalPlayer-iOS"
  xcrun ibtool \
    --minimum-deployment-target 16.0 \
    --target-device iphone \
    --target-device ipad \
    --compile "$APP_BUNDLE/LaunchScreen.storyboardc" \
    "$LAUNCH_SCREEN_SOURCE"
  write_info_plist
  xattr -cr "$APP_BUNDLE"
  codesign --force --sign - --timestamp=none "$APP_BUNDLE"
}

boot_simulator() {
  local udid="$1"

  xcrun simctl boot "$udid" >/dev/null 2>&1 || true
  xcrun simctl bootstatus "$udid" -b
  open -a Simulator --args -CurrentDeviceUDID "$udid"
}

seed_fixture_files() {
  local udid="$1"
  local data_container
  local fixture_dest

  data_container="$(xcrun simctl get_app_container "$udid" "$SIM_BUNDLE_ID" data)"
  fixture_dest="$data_container/Documents/$FIXTURE_DEST_NAME"

  rm -f "$data_container/Documents/import-debug.log"
  rm -rf "$fixture_dest"
  mkdir -p "$fixture_dest"
  cp -R "$FIXTURE_SOURCE"/. "$fixture_dest"/

  echo "Seeded audio fixtures:"
  find "$fixture_dest" -maxdepth 1 -type f -print
}

main() {
  require_sim_runtime

  local udid
  udid="$(find_sim_udid)"
  if [[ -z "$udid" ]]; then
    echo "error: simulator device '${SIM_DEVICE_NAME}' was not found." >&2
    echo "Available iPhone devices:" >&2
    xcrun simctl list devices available | sed -n '/-- iOS /,/^$/p' >&2
    exit 1
  fi

  if [[ ! -d "$FIXTURE_SOURCE" ]]; then
    echo "error: fixture directory not found: $FIXTURE_SOURCE" >&2
    exit 1
  fi

  ensure_xcframework
  build_swift_product
  package_app_bundle
  boot_simulator "$udid"

  xcrun simctl uninstall "$udid" "$SIM_BUNDLE_ID" >/dev/null 2>&1 || true
  xcrun simctl install "$udid" "$APP_BUNDLE"
  seed_fixture_files "$udid"
  xcrun simctl launch "$udid" "$SIM_BUNDLE_ID"

  cat <<EOF

NormalPlayer is running on ${SIM_DEVICE_NAME}.

To test real file import in the simulator:
1. Tap the top-left menu button in NormalPlayer.
2. Choose Import Files.
3. In the Files picker, open Browse > On My iPhone > NormalPlayer.
4. Open the ${FIXTURE_DEST_NAME} folder.
5. Tap Select, choose the .ogg files, and confirm.

EOF
}

main "$@"
