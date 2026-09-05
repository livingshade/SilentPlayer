#if os(macOS)
import AppKit
import SwiftUI

public enum MacDashboardReopenPolicy {
    public static func shouldRestoreDashboard(
        eventWindowIdentifier: String?
    ) -> Bool {
        eventWindowIdentifier != FloatingLyricsWindowState.windowIdentifier
    }
}

@MainActor
public final class MacDashboardWindowCoordinator {
    private weak var window: NSWindow?

    public init() {}

    public func attach(to window: NSWindow) {
        self.window = window
    }

    public func restoreDashboardForReopen() -> Bool {
        guard let window else {
            return false
        }
        if window.isMiniaturized {
            window.deminiaturize(nil)
        }
        window.makeKeyAndOrderFront(nil)
        return true
    }
}

public struct MacDashboardWindowAttachment: NSViewRepresentable {
    private let coordinator: MacDashboardWindowCoordinator

    public init(coordinator: MacDashboardWindowCoordinator) {
        self.coordinator = coordinator
    }

    public func makeNSView(context: Context) -> NSView {
        DashboardWindowAttachmentView(coordinator: coordinator)
    }

    public func updateNSView(_ nsView: NSView, context: Context) {}
}

@MainActor
private final class DashboardWindowAttachmentView: NSView {
    private let coordinator: MacDashboardWindowCoordinator

    init(coordinator: MacDashboardWindowCoordinator) {
        self.coordinator = coordinator
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is unavailable")
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let window {
            coordinator.attach(to: window)
        }
    }
}
#endif
