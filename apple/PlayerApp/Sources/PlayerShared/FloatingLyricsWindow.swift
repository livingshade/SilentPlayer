#if os(macOS)
import AppKit
import SwiftUI

@MainActor
public final class FloatingLyricsWindowState: ObservableObject {
    @Published public private(set) var isLocked = false
    @Published public private(set) var isHovering = false
    private weak var window: NSWindow?

    public var showsOverlayControls: Bool {
        isHovering && !isLocked
    }

    public init() {}

    func attach(to window: NSWindow) {
        self.window = window
        window.styleMask = [.borderless]
        window.titleVisibility = .hidden
        window.toolbar = nil
        window.level = .floating
        window.hidesOnDeactivate = false
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = false
        window.collectionBehavior.formUnion([
            .canJoinAllSpaces,
            .fullScreenAuxiliary,
            .ignoresCycle
        ])
        applyInteractionState()
    }

    public func setLocked(_ locked: Bool) {
        isLocked = locked
        applyInteractionState()
    }

    public func toggleLocked() {
        setLocked(!isLocked)
    }

    public func setHovering(_ hovering: Bool) {
        isHovering = hovering
    }

    private func applyInteractionState() {
        window?.ignoresMouseEvents = isLocked
        window?.isMovable = !isLocked
        window?.isMovableByWindowBackground = !isLocked
    }
}

enum FloatingLyricsPresentation {
    static func currentLine(
        document: LyricsDocument?,
        fallbackText: String?,
        positionMS: Int,
        isLoading: Bool
    ) -> String {
        if let document {
            if document.timedLines != nil {
                return document.compactLine(at: positionMS) ?? "…"
            }
            return document.compactLine() ?? document.instrumentalToken
        }
        if let fallbackText,
           let firstLine = fallbackText
               .components(separatedBy: .newlines)
               .map({ $0.trimmingCharacters(in: .whitespacesAndNewlines) })
               .first(where: { !$0.isEmpty }) {
            return firstLine
        }
        return isLoading
            ? "Loading lyrics…"
            : LyricsDocument.defaultInstrumentalToken
    }
}

public struct FloatingLyricsOpenButton: View {
    @Environment(\.openWindow) private var openWindow

    public init() {}

    public var body: some View {
        Button {
            openWindow(id: "floating-lyrics")
        } label: {
            Label("Lyrics", systemImage: "text.quote")
                .font(.callout.weight(.semibold))
        }
        .buttonStyle(.bordered)
        .controlSize(.regular)
        .help("Show Floating Lyrics")
    }
}

public struct FloatingLyricsWindowContent: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject private var model: AppModel
    @ObservedObject private var windowState: FloatingLyricsWindowState

    public init(model: AppModel, windowState: FloatingLyricsWindowState) {
        self.model = model
        self.windowState = windowState
    }

    public var body: some View {
        ZStack(alignment: .topTrailing) {
            VStack(spacing: 7) {
                Text(model.playback.nowPlaying?.title ?? "Not Playing")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)

                TimelineView(
                    .periodic(
                        from: .now,
                        by: model.playback.isPlaying ? 0.2 : 1
                    )
                ) { _ in
                    Text(currentLyrics)
                        .font(.title3.weight(.semibold))
                        .lineLimit(1)
                        .minimumScaleFactor(0.72)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: .infinity)
                }
            }
            .padding(.horizontal, 72)
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            HStack(spacing: 6) {
                Button {
                    windowState.setLocked(true)
                } label: {
                    Label("Lock Lyrics", systemImage: "lock")
                        .labelStyle(.iconOnly)
                }
                .help("Lock Lyrics")

                Button {
                    dismiss()
                } label: {
                    Label("Close Lyrics", systemImage: "xmark")
                        .labelStyle(.iconOnly)
                }
                .help("Close Lyrics")
            }
            .buttonStyle(.glass)
            .controlSize(.small)
            .padding(10)
            .opacity(windowState.showsOverlayControls ? 1 : 0)
            .allowsHitTesting(windowState.showsOverlayControls)
        }
        .frame(width: 560, height: 96)
        .contentShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
        .glassEffect(
            .regular,
            in: RoundedRectangle(cornerRadius: 24, style: .continuous)
        )
        .onHover { hovering in
            windowState.setHovering(hovering)
        }
        .gesture(WindowDragGesture())
        .allowsWindowActivationEvents()
        .animation(.easeInOut(duration: 0.16), value: windowState.showsOverlayControls)
        .background {
            FloatingLyricsWindowBridge(windowState: windowState)
                .frame(width: 0, height: 0)
        }
    }

    private var currentLyrics: String {
        let details = model.playback.details
        return FloatingLyricsPresentation.currentLine(
            document: details?.lyricsDocument,
            fallbackText: details?.lyricsText,
            positionMS: model.estimatedPlaybackPositionMS(),
            isLoading: model.playback.isLoadingDetails
        )
    }
}

private struct FloatingLyricsWindowBridge: NSViewRepresentable {
    @ObservedObject var windowState: FloatingLyricsWindowState

    func makeNSView(context: Context) -> FloatingLyricsWindowAttachmentView {
        FloatingLyricsWindowAttachmentView(windowState: windowState)
    }

    func updateNSView(
        _ nsView: FloatingLyricsWindowAttachmentView,
        context: Context
    ) {
        nsView.windowState = windowState
        if let window = nsView.window {
            windowState.attach(to: window)
        }
    }
}

@MainActor
private final class FloatingLyricsWindowAttachmentView: NSView {
    var windowState: FloatingLyricsWindowState

    init(windowState: FloatingLyricsWindowState) {
        self.windowState = windowState
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is unavailable")
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let window {
            windowState.attach(to: window)
        }
    }
}
#endif
