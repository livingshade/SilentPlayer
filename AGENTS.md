# NormalPlayer Agent Instructions

## Required Development Order

When adding or changing product functionality, especially anything that touches both playback/library behavior and the app UI, follow this order:

1. Implement and verify the Rust layer first.
2. Add or update Rust tests that cover the new behavior, including integration or corner-case coverage when the behavior crosses storage, FFI, playback, metadata, view identity, analysis, import, or history boundaries.
3. Run the relevant Rust tests and make sure they pass before changing the UI layer.
4. Only after the Rust layer is correct and tested, update the SwiftUI/macOS UI.
5. Build and test the Swift layer after UI changes.

Do not use the CLI as the app integration model. The CLI is only for Rust-layer debugging and smoke testing. The UI should call stable Rust/FFI APIs designed for app use.

## UI Design Standard

For SwiftUI/macOS and future iOS UI work, design against official Apple documentation and best practices rather than ad hoc layout guesses. Use Apple Human Interface Guidelines, official SwiftUI documentation, and platform-appropriate controls, navigation, sizing, and accessibility patterns when making meaningful UI or layout changes.

When a UI issue depends on platform behavior, verify the relevant official documentation before implementing the fix, then validate the result with a local build and, when practical, by running the app.
