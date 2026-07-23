# Silent SwiftUI App

This is the SwiftUI shell for the local player. It is a Swift Package, but the macOS executable also needs the Rust FFI dynamic library. For local app testing, use the repo script so the matching debug Rust library and worker executables are built, linked through `@rpath`, and copied into the app bundle:

```bash
../../scripts/run_mac_swiftui.sh
```

The Swift package maps Debug builds to `target/debug/libplayer_ffi.dylib` and Release builds to `target/release/libplayer_ffi.dylib`. Use the script instead of a bare `swift run Silent`; the script prepares the dynamic-library install name before Swift links the executable.
Set `SILENT_SKIP_OPEN=1` when only a build-and-bundle verification is needed.

The macOS executable uses shared SwiftUI view/model code in `PlayerShared` and talks to Rust through the `player_ffi` C ABI. Silent's generic CLI is a first-class third target that uses the same Rust application behavior as macOS and iPhone. The app stores managed audio copies under `~/Music/NormalPlayer/Music` and persistent SQLite state, including loudness analysis results, at `~/Music/NormalPlayer/player_library.sqlite3`.

## iPhone App

The package also contains an iOS entry point:

- `Sources/PlayeriOS/PlayeriOSApp.swift`
- `Sources/PlayerShared/PhoneContentView.swift`

The iPhone UI uses a compact tab-based layout and the same `AppModel`/Rust FFI API as macOS. It does not use the macOS background worker executables because iOS apps should not depend on spawning bundled helper processes; import, analyze, and audit use the direct Rust FFI calls instead.

The iOS entry point installs an Apple playback-system integration that configures an
`AVAudioSession` for long-form playback, publishes lock-screen metadata through
`MPNowPlayingInfoCenter`, handles play/pause/next/previous/seek/repeat/shuffle remote
commands, pauses when an audio output disconnects, and coordinates interruption resume
decisions with the Rust playback lifecycle state machine. The simulator packaging script
adds the `audio` background mode to the generated app `Info.plist`.

Building the iOS app requires the full Xcode install, not Command Line Tools only. First build the Rust static libraries and generate the ignored local XCFramework:

```bash
../../scripts/build_ios_rust.sh
```

The generated `Vendor/PlayerFFI.xcframework` is intentionally excluded from Git because
compiled Rust archives can contain machine-specific build paths. Run the script before
opening the package in Xcode, then build the `NormalPlayer-iOS` product for a simulator
or device. A machine with Command Line Tools only can still use `swift test` to validate
the shared/macOS build and a macOS stub for `NormalPlayer-iOS`, but it cannot type-check
or launch the real iOS SwiftUI app until the iPhone SDKs are installed.

For local simulator testing, use the repo script:

```bash
../../scripts/run_ios_simulator.sh
```

The script builds the Rust iOS libraries, regenerates `Vendor/PlayerFFI.xcframework`, cross-compiles the `NormalPlayer-iOS` Swift product with the iPhone Simulator SDK, packages a simulator `.app`, installs it on the configured simulator, and seeds `test-assets/audio` into the app's Documents folder.

To test real file import in the simulator:

1. Run `../../scripts/run_ios_simulator.sh`.
2. In NormalPlayer, tap the top-left menu button.
3. Choose `Import Files`.
4. In the Files picker, open `Browse > On My iPhone > NormalPlayer`.
5. Open `ImportFixtures`.
6. Tap `Select`, choose the `.ogg` files, and confirm.

The app bundle used by the script enables `UIFileSharingEnabled` and `LSSupportsOpeningDocumentsInPlace`, so the seeded fixture folder is visible through Files instead of being imported through a test-only backdoor.
