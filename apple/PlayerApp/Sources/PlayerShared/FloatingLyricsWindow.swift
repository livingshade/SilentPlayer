#if os(macOS)
import AppKit
import SwiftUI

@MainActor
public final class FloatingLyricsWindowState: ObservableObject {
    nonisolated public static let windowIdentifier = "floating-lyrics"

    @Published public private(set) var isLocked = false
    @Published public private(set) var isHovering = false
    private weak var window: NSWindow?
    private var contentSize: NSSize?

    public var showsOverlayControls: Bool {
        isHovering && !isLocked
    }

    public init() {}

    func attach(to window: NSWindow) {
        self.window = window
        window.identifier = NSUserInterfaceItemIdentifier(Self.windowIdentifier)
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
        applyContentSize()
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

    public func setContentSize(_ size: NSSize) {
        guard contentSize != size else {
            return
        }
        contentSize = size
        applyContentSize()
    }

    private func applyInteractionState() {
        window?.ignoresMouseEvents = isLocked
        window?.isMovable = !isLocked
        window?.isMovableByWindowBackground = !isLocked
    }

    private func applyContentSize() {
        guard let window, let contentSize,
              window.contentLayoutRect.size != contentSize else {
            return
        }
        window.setContentSize(contentSize)
    }
}

@MainActor
enum FloatingLyricsWindowLayout {
    static let minimumWidth: CGFloat = 320
    static let maximumWidth: CGFloat = 720
    static let horizontalPadding: CGFloat = 48
    static let contentHeight: CGFloat = 124
    static let lyricsFont = NSFont.systemFont(ofSize: 26, weight: .semibold)

    static func width(longestLineWidth: CGFloat) -> CGFloat {
        min(
            maximumWidth,
            max(minimumWidth, ceil(longestLineWidth + horizontalPadding))
        )
    }

    static func width(
        document: LyricsDocument?,
        fallbackText: String?
    ) -> CGFloat {
        let lines = presentationLines(document: document, fallbackText: fallbackText)
        let longest = lines
            .map {
                RubyLyricsTextLayout.typographicWidth(
                    for: $0,
                    font: lyricsFont
                )
            }
            .max() ?? 0
        return width(longestLineWidth: longest)
    }

    private static func presentationLines(
        document: LyricsDocument?,
        fallbackText: String?
    ) -> [LyricsPresentationLine] {
        if let document {
            switch document.content {
            case .timed(let lines):
                return lines.map {
                    LyricsPresentationLine(text: $0.text, ruby: $0.ruby)
                }
            case .plain(let text):
                return plainLines(text)
            case .instrumental:
                return [LyricsPresentationLine(
                    text: document.instrumentalToken,
                    ruby: []
                )]
            }
        }
        if let fallbackText {
            return plainLines(fallbackText)
        }
        return [LyricsPresentationLine(
            text: LyricsDocument.defaultInstrumentalToken,
            ruby: []
        )]
    }

    private static func plainLines(_ text: String) -> [LyricsPresentationLine] {
        text.components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .map { LyricsPresentationLine(text: $0, ruby: []) }
    }
}

enum FloatingLyricsPresentation {
    static func currentLine(
        document: LyricsDocument?,
        fallbackText: String?,
        positionMS: Int,
        isLoading: Bool
    ) -> LyricsPresentationLine {
        if let document {
            if let lines = document.timedLines {
                guard let activeIndex = document.activeLineIndex(at: positionMS) else {
                    return LyricsPresentationLine(
                        text: document.instrumentalToken,
                        ruby: []
                    )
                }
                let line = lines[activeIndex]
                guard !line.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                    return LyricsPresentationLine(
                        text: document.instrumentalToken,
                        ruby: []
                    )
                }
                return LyricsPresentationLine(text: line.text, ruby: line.ruby)
            }
            return LyricsPresentationLine(
                text: document.compactLine() ?? document.instrumentalToken,
                ruby: []
            )
        }
        if let fallbackText,
           let firstLine = fallbackText
               .components(separatedBy: .newlines)
               .map({ $0.trimmingCharacters(in: .whitespacesAndNewlines) })
               .first(where: { !$0.isEmpty }) {
            return LyricsPresentationLine(text: firstLine, ruby: [])
        }
        return LyricsPresentationLine(
            text: isLoading ? "Loading lyrics…" : LyricsDocument.defaultInstrumentalToken,
            ruby: []
        )
    }
}

struct LyricsPresentationLine: Hashable {
    let text: String
    let ruby: [LyricsRubyAnnotation]
}

public struct FloatingLyricsOpenButton: View {
    @Environment(\.openWindow) private var openWindow

    public init() {}

    public var body: some View {
        Button {
            openWindow(id: FloatingLyricsWindowState.windowIdentifier)
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
        let details = model.playback.details
        let contentWidth = FloatingLyricsWindowLayout.width(
            document: details?.lyricsDocument,
            fallbackText: details?.lyricsText
        )
        let contentSize = NSSize(
            width: contentWidth,
            height: FloatingLyricsWindowLayout.contentHeight
        )

        ZStack(alignment: .topTrailing) {
            VStack(spacing: 12) {
                Text(model.playback.nowPlaying?.title ?? "Not Playing")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .frame(height: 16)

                TimelineView(
                    .periodic(
                        from: .now,
                        by: model.playback.isPlaying ? 0.2 : 1
                    )
                ) { _ in
                    RubyLyricsText(
                        line: currentLyrics,
                        font: FloatingLyricsWindowLayout.lyricsFont,
                        color: .labelColor
                    )
                    .frame(maxWidth: .infinity, minHeight: 54, maxHeight: 54)
                }
            }
            .padding(.horizontal, FloatingLyricsWindowLayout.horizontalPadding / 2)
            .padding(.vertical, 12)
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
        .frame(width: contentSize.width, height: contentSize.height)
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
            FloatingLyricsWindowBridge(
                windowState: windowState,
                contentSize: contentSize
            )
                .frame(width: 0, height: 0)
        }
    }

    private var currentLyrics: LyricsPresentationLine {
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
    let contentSize: NSSize

    func makeNSView(context: Context) -> FloatingLyricsWindowAttachmentView {
        FloatingLyricsWindowAttachmentView(
            windowState: windowState,
            contentSize: contentSize
        )
    }

    func updateNSView(
        _ nsView: FloatingLyricsWindowAttachmentView,
        context: Context
    ) {
        nsView.windowState = windowState
        nsView.contentSize = contentSize
        windowState.setContentSize(contentSize)
        if let window = nsView.window {
            windowState.attach(to: window)
        }
    }
}

@MainActor
private final class FloatingLyricsWindowAttachmentView: NSView {
    var windowState: FloatingLyricsWindowState
    var contentSize: NSSize

    init(windowState: FloatingLyricsWindowState, contentSize: NSSize) {
        self.windowState = windowState
        self.contentSize = contentSize
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
            windowState.setContentSize(contentSize)
        }
    }
}
#endif
