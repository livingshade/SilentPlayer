#if os(macOS)
import AppKit
import XCTest
@testable import PlayerShared

@MainActor
final class MacDashboardWindowTests: XCTestCase {
    func testReopenOrdersAttachedDashboardFront() {
        let coordinator = MacDashboardWindowCoordinator()
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 600),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.orderOut(nil)
        coordinator.attach(to: window)

        XCTAssertTrue(coordinator.restoreDashboardForReopen())
        XCTAssertTrue(window.isVisible)
    }

    func testReopenWithoutDashboardDefersToSwiftUI() {
        let coordinator = MacDashboardWindowCoordinator()

        XCTAssertFalse(coordinator.restoreDashboardForReopen())
    }

    func testFloatingLyricsWindowActivationDoesNotRestoreDashboard() {
        XCTAssertFalse(
            MacDashboardReopenPolicy.shouldRestoreDashboard(
                eventWindowIdentifier: FloatingLyricsWindowState.windowIdentifier
            )
        )
    }

    func testFloatingLyricsPanelDoesNotActivateOwningApplication() {
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 320, height: 124),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        let state = FloatingLyricsWindowState()

        state.attach(to: panel)

        XCTAssertTrue(panel.styleMask.contains(.nonactivatingPanel))
        XCTAssertTrue(panel.becomesKeyOnlyIfNeeded)
    }

    func testDockReopenRestoresDashboard() {
        XCTAssertTrue(
            MacDashboardReopenPolicy.shouldRestoreDashboard(
                eventWindowIdentifier: nil
            )
        )
    }
}
#endif
