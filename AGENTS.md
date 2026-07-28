# Silent Agent Instructions

## Required Development Order

When adding or changing product functionality, especially anything that touches both playback/library behavior and the app UI, follow this order:

1. Implement and verify the Rust layer first.
2. Add or update Rust tests that cover the new behavior, including integration or corner-case coverage when the behavior crosses storage, FFI, playback, metadata, view identity, analysis, import, or history boundaries.
3. Run the relevant Rust tests and make sure they pass before changing the UI layer.
4. Only after the Rust layer is correct and tested, update the SwiftUI/macOS UI.
5. Build and test the Swift layer after UI changes.

Treat the CLI as a first-class third product target alongside macOS and iPhone. Shared product
behavior must live in Rust application services that all three targets use. The CLI must not
reimplement product rules by editing SQLite directly when an application service exists. Apple
UI targets should continue to call stable Rust/FFI APIs designed for app use.

## UI Design Standard

For SwiftUI/macOS and future iOS UI work, design against official Apple documentation and best practices rather than ad hoc layout guesses. Use Apple Human Interface Guidelines, official SwiftUI documentation, and platform-appropriate controls, navigation, sizing, and accessibility patterns when making meaningful UI or layout changes.

When a UI issue depends on platform behavior, verify the relevant official documentation before implementing the fix, then validate the result with a local build and, when practical, by running the app.

## macOS App Installation

After every macOS app update:

1. Build and verify the new `Silent.app` bundle first.
2. Quit any running installed copy of Silent.
3. Delete the existing `/Applications/Silent.app` bundle completely. Do not merge or copy the new bundle over the old bundle.
4. Remove stale distributable copies such as `dist/Silent.app` when they are not the newly built bundle being installed. Do not leave an outdated app bundle that Finder, Spotlight, or Launch Services can present as a second Silent installation. The current hidden build-staging bundle under `.build` may remain.
5. Install the newly built bundle at `/Applications/Silent.app`.
6. Launch the installed copy and verify that it is the updated build.
7. Before reporting completion, verify that system application search resolves Silent to the single installed path `/Applications/Silent.app`.

## Git Status Reporting

After every commit or push:

1. Report the current local branch name.
2. Report whether the committed or pushed changes have been merged into the repository's default branch.
3. Use the repository's actual default branch name in the report. For this repository, the default branch is currently `master`; do not describe it as `main` unless the repository is renamed.
