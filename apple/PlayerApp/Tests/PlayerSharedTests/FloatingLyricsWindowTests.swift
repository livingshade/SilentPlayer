#if os(macOS)
import AppKit
import XCTest
@testable import PlayerShared

@MainActor
final class FloatingLyricsWindowTests: XCTestCase {
    func testLockMakesTheAttachedWindowClickThroughAndImmovable() {
        let state = FloatingLyricsWindowState()
        let window = makeNativeWindow()
        state.attach(to: window)

        state.setLocked(true)

        XCTAssertTrue(state.isLocked)
        XCTAssertTrue(window.ignoresMouseEvents)
        XCTAssertFalse(window.isMovable)
        XCTAssertFalse(window.isMovableByWindowBackground)
    }

    func testUnlockRestoresWindowInteractionAndDragging() {
        let state = FloatingLyricsWindowState()
        let window = makeNativeWindow()
        state.attach(to: window)
        state.setLocked(true)

        state.setLocked(false)

        XCTAssertFalse(state.isLocked)
        XCTAssertFalse(window.ignoresMouseEvents)
        XCTAssertTrue(window.isMovable)
        XCTAssertTrue(window.isMovableByWindowBackground)
    }

    func testControlsAppearOnlyWhileHoveringUnlockedWindow() {
        let state = FloatingLyricsWindowState()

        XCTAssertFalse(state.showsOverlayControls)

        state.setHovering(true)
        XCTAssertTrue(state.showsOverlayControls)

        state.setHovering(false)
        XCTAssertFalse(state.showsOverlayControls)
    }

    func testPresentationShowsCurrentTimedLineAtPlaybackPosition() {
        let document = LyricsDocument(
            format: .lrc,
            content: .timed([
                TimedLyricsLine(id: 0, startMS: 1_000, text: "First"),
                TimedLyricsLine(id: 1, startMS: 2_500, text: "Second")
            ])
        )

        XCTAssertEqual(
            FloatingLyricsPresentation.currentLine(
                document: document,
                fallbackText: nil,
                positionMS: 2_600,
                isLoading: false
            ),
            "Second"
        )
    }

    func testPresentationUsesFirstNonblankFallbackLine() {
        XCTAssertEqual(
            FloatingLyricsPresentation.currentLine(
                document: nil,
                fallbackText: "  \n First line \nSecond line",
                positionMS: 0,
                isLoading: false
            ),
            "First line"
        )
    }

    func testPresentationShowsLoadingStateBeforeLyricsArrive() {
        XCTAssertEqual(
            FloatingLyricsPresentation.currentLine(
                document: nil,
                fallbackText: nil,
                positionMS: 0,
                isLoading: true
            ),
            "Loading lyrics…"
        )
    }

    func testPresentationUsesInstrumentalTokenWhenLyricsAreUnavailable() {
        XCTAssertEqual(
            FloatingLyricsPresentation.currentLine(
                document: nil,
                fallbackText: nil,
                positionMS: 0,
                isLoading: false
            ),
            "♪"
        )
    }

    func testLockImmediatelyHidesHoverControls() {
        let state = FloatingLyricsWindowState()
        state.setHovering(true)

        state.setLocked(true)

        XCTAssertFalse(state.showsOverlayControls)
    }

    private func makeNativeWindow() -> NSWindow {
        NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 560, height: 100),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
    }
}
#endif
