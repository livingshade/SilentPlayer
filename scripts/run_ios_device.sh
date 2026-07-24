#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

: "${NORMALPLAYER_IOS_DEVICE_UDID:?Set NORMALPLAYER_IOS_DEVICE_UDID to the connected iPhone UDID}"
: "${NORMALPLAYER_IOS_COREDEVICE_ID:?Set NORMALPLAYER_IOS_COREDEVICE_ID from xcrun devicectl list devices}"
: "${NORMALPLAYER_IOS_PROFILE:?Set NORMALPLAYER_IOS_PROFILE to a matching .mobileprovision file}"
: "${NORMALPLAYER_IOS_SIGNING_IDENTITY:?Set NORMALPLAYER_IOS_SIGNING_IDENTITY to an Apple Development identity}"

REPO_ROOT="$(pwd)"
APP_ROOT="$REPO_ROOT/apple/PlayerApp"
DERIVED_DATA="$APP_ROOT/.derived-ios-device"
PRODUCT_DIR="$DERIVED_DATA/Build/Products/Debug-iphoneos"
APP_BUNDLE="$PRODUCT_DIR/NormalPlayer-iOS.app"
RAW_BINARY="$PRODUCT_DIR/NormalPlayer-iOS"
LAUNCH_SCREEN_SOURCE="$APP_ROOT/Resources/LaunchScreen.storyboard"
APP_ICON_CATALOG="$APP_ROOT/Resources/AppIcon.xcassets"
BUNDLE_ID="${NORMALPLAYER_IOS_BUNDLE_ID:-com.normalplayer.ios}"
PROFILE_PLIST="$PRODUCT_DIR/NormalPlayer-Profile.plist"
ENTITLEMENTS="$PRODUCT_DIR/NormalPlayer.entitlements"

build_device_binary() {
  ./scripts/build_ios_rust.sh
  (
    cd "$APP_ROOT"
    xcodebuild \
      -scheme NormalPlayer-iOS \
      -destination "id=$NORMALPLAYER_IOS_DEVICE_UDID" \
      -configuration Debug \
      -derivedDataPath "$DERIVED_DATA" \
      build
  )
}

write_info_plist() {
  local plist="$APP_BUNDLE/Info.plist"

  plutil -create xml1 "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleExecutable string NormalPlayer-iOS" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string $BUNDLE_ID" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleName string Silent" "$plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleDisplayName string Silent" "$plist"
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

compile_resources() {
  local partial_plist="$PRODUCT_DIR/AppIcon-Device-Info.plist"

  xcrun ibtool \
    --minimum-deployment-target 16.0 \
    --target-device iphone \
    --target-device ipad \
    --compile "$APP_BUNDLE/LaunchScreen.storyboardc" \
    "$LAUNCH_SCREEN_SOURCE"
  xcrun actool \
    "$APP_ICON_CATALOG" \
    --compile "$APP_BUNDLE" \
    --platform iphoneos \
    --minimum-deployment-target 16.0 \
    --target-device iphone \
    --target-device ipad \
    --app-icon AppIcon \
    --output-partial-info-plist "$partial_plist"
  /usr/libexec/PlistBuddy -c "Merge $partial_plist" "$APP_BUNDLE/Info.plist"
}

package_and_sign() {
  if [[ ! -x "$RAW_BINARY" ]]; then
    echo "error: expected device binary at $RAW_BINARY" >&2
    exit 1
  fi
  if [[ ! -f "$NORMALPLAYER_IOS_PROFILE" ]]; then
    echo "error: provisioning profile not found: $NORMALPLAYER_IOS_PROFILE" >&2
    exit 1
  fi

  rm -rf "$APP_BUNDLE"
  mkdir -p "$APP_BUNDLE"
  cp "$RAW_BINARY" "$APP_BUNDLE/NormalPlayer-iOS"
  cp "$NORMALPLAYER_IOS_PROFILE" "$APP_BUNDLE/embedded.mobileprovision"
  write_info_plist
  compile_resources

  security cms -D -i "$NORMALPLAYER_IOS_PROFILE" > "$PROFILE_PLIST"
  /usr/libexec/PlistBuddy -x -c "Print :Entitlements" "$PROFILE_PLIST" > "$ENTITLEMENTS"
  xattr -cr "$APP_BUNDLE"
  codesign \
    --force \
    --sign "$NORMALPLAYER_IOS_SIGNING_IDENTITY" \
    --entitlements "$ENTITLEMENTS" \
    --generate-entitlement-der \
    --timestamp=none \
    "$APP_BUNDLE"
  codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"
}

install_and_launch() {
  xcrun devicectl device install app \
    --device "$NORMALPLAYER_IOS_COREDEVICE_ID" \
    "$APP_BUNDLE"
  xcrun devicectl device process launch \
    --device "$NORMALPLAYER_IOS_COREDEVICE_ID" \
    "$BUNDLE_ID"
}

build_device_binary
package_and_sign
install_and_launch

echo "Installed and launched $BUNDLE_ID on $NORMALPLAYER_IOS_DEVICE_UDID"
