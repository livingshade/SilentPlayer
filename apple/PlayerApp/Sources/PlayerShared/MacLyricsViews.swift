#if os(macOS)
import Foundation
import SwiftUI

struct CompactLyricsView: View {
    @ObservedObject var model: AppModel
    let track: TrackItem?
    let document: LyricsDocument?
    let fallbackText: String?
    let isLoading: Bool

    var body: some View {
        Group {
            if let document, document.hasDisplayableLyrics {
                if document.timedLines != nil,
                   let track,
                   model.playback.nowPlaying?.id == track.id {
                    TimelineView(.periodic(from: .now, by: model.playback.isPlaying ? 0.2 : 1)) { _ in
                        lyricLine(document.compactLine(at: model.estimatedPlaybackPositionMS()))
                    }
                } else {
                    lyricLine(document.compactLine())
                }
            } else if let fallbackLine {
                lyricLine(fallbackLine)
            } else if isLoading {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Loading lyrics…")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, minHeight: 62)
            } else {
                NoLyricsState(
                    compact: true,
                    token: document?.instrumentalToken
                        ?? LyricsDocument.defaultInstrumentalToken
                )
            }
        }
        .frame(maxWidth: .infinity, minHeight: 62)
        .background(Color(nsColor: .textBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 6))
        .overlay {
            RoundedRectangle(cornerRadius: 6)
                .stroke(Color(nsColor: .separatorColor).opacity(0.28), lineWidth: 1)
        }
    }

    private func lyricLine(_ line: String?) -> some View {
        Text(line ?? "…")
            .font(.callout.weight(.medium))
            .lineLimit(1)
            .minimumScaleFactor(0.82)
            .frame(maxWidth: .infinity, minHeight: 62, alignment: .center)
            .padding(.horizontal, 16)
            .accessibilityLabel(
                line == document?.instrumentalToken
                    ? "Instrumental"
                    : (line ?? "Waiting for lyrics")
            )
    }

    private var fallbackLine: String? {
        guard let fallbackText else {
            return nil
        }
        return fallbackText
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty }
    }
}

struct NoLyricsState: View {
    var compact = false
    var token = LyricsDocument.defaultInstrumentalToken

    var body: some View {
        Text(token)
            .font(compact ? .title2.weight(.medium) : .largeTitle)
            .foregroundStyle(.secondary)
            .frame(
                maxWidth: .infinity,
                minHeight: compact ? 62 : nil,
                maxHeight: compact ? nil : .infinity
            )
            .accessibilityLabel("Instrumental")
    }
}

struct NowPlayingLyricsView: View {
    @ObservedObject var model: AppModel
    let document: LyricsDocument?
    let fallbackText: String?
    let isLoading: Bool

    var body: some View {
        Group {
            if let document {
                switch document.content {
                case .timed(let lines):
                    if lines.isEmpty {
                        emptyLyrics
                    } else {
                        TimedLyricsView(model: model, document: document)
                    }
                case .plain(let text):
                    if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        emptyLyrics
                    } else {
                        plainLyrics(text)
                    }
                case .instrumental:
                    emptyLyrics
                }
            } else if let fallbackText,
                      !fallbackText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                plainLyrics(fallbackText)
            } else if isLoading {
                VStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Loading lyrics…")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                emptyLyrics
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func plainLyrics(_ text: String) -> some View {
        ScrollView {
            Text(text)
                .font(.title3)
                .lineSpacing(6)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 20)
                .padding(.vertical, 42)
        }
    }

    private var emptyLyrics: some View {
        NoLyricsState(
            token: document?.instrumentalToken
                ?? LyricsDocument.defaultInstrumentalToken
        )
    }
}

struct TimedLyricsView: View {
    @ObservedObject var model: AppModel
    let document: LyricsDocument
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var followsPlayback = true

    private var lines: [TimedLyricsLine] {
        document.timedLines ?? []
    }

    private var presentationLines: [TimedLyricsLine] {
        guard let first = lines.first, first.startMS > 0 else {
            return lines
        }
        return [TimedLyricsLine(id: -1, startMS: 0, text: "")] + lines
    }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.2)) { _ in
            let positionMS = model.estimatedPlaybackPositionMS()
            let activeIndex = document.activeLineIndex(at: positionMS)
            lyricsScroller(activeIndex: activeIndex)
        }
    }

    private func lyricsScroller(activeIndex: Int?) -> some View {
        let hasInstrumentalPrelude = lines.first?.startMS ?? 0 > 0
        let activeID = activeIndex.map { lines[$0].id }
            ?? (hasInstrumentalPrelude ? -1 : nil)
        return ScrollViewReader { proxy in
            ZStack(alignment: .bottomTrailing) {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 20) {
                        ForEach(presentationLines) { line in
                            lyricButton(
                                line,
                                isActive: line.id == activeID
                            )
                        }
                    }
                    .padding(.horizontal, 20)
                    .padding(.vertical, 56)
                }
                .simultaneousGesture(
                    DragGesture(minimumDistance: 3)
                        .onChanged { _ in
                            followsPlayback = false
                        }
                )

                if !followsPlayback, let activeID {
                    Button {
                        followsPlayback = true
                        scroll(to: activeID, using: proxy)
                    } label: {
                        Label("Follow Lyrics", systemImage: "location.fill")
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .padding(10)
                }
            }
            .onAppear {
                guard followsPlayback, let activeID else {
                    return
                }
                DispatchQueue.main.async {
                    scroll(to: activeID, using: proxy)
                }
            }
            .onChange(of: activeID) { newID in
                guard followsPlayback, let newID else {
                    return
                }
                scroll(to: newID, using: proxy)
            }
        }
    }

    private func lyricButton(
        _ line: TimedLyricsLine,
        isActive: Bool
    ) -> some View {
        let trimmedText = line.text.trimmingCharacters(in: .whitespacesAndNewlines)
        let isInstrumental = trimmedText.isEmpty
        return Button {
            followsPlayback = true
            Task { await model.seek(toMilliseconds: line.startMS) }
        } label: {
            Text(isInstrumental ? document.instrumentalToken : line.text)
                .font(.title3.weight(isActive ? .semibold : .regular))
                .foregroundStyle(isActive ? Color.primary : Color.secondary)
                .multilineTextAlignment(.leading)
                .frame(maxWidth: .infinity, minHeight: 30, alignment: .leading)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(isInstrumental ? "Instrumental" : line.text)
        .accessibilityValue(isActive ? "Current lyric" : "")
        .help("Seek to \(formatLyricsTime(line.startMS))")
    }

    private func scroll(to id: Int, using proxy: ScrollViewProxy) {
        if reduceMotion {
            proxy.scrollTo(id, anchor: .center)
        } else {
            withAnimation(.easeInOut(duration: 0.24)) {
                proxy.scrollTo(id, anchor: .center)
            }
        }
    }

    private func formatLyricsTime(_ milliseconds: Int) -> String {
        let totalSeconds = max(0, milliseconds) / 1_000
        return String(format: "%d:%02d", totalSeconds / 60, totalSeconds % 60)
    }
}
#endif
