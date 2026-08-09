#if os(iOS)
import Foundation
import SwiftUI
import UIKit

struct PhoneLyricsNotesPanel: View {
    let details: TrackDetails?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            if let lyricsText = details?.lyricsText,
               !lyricsText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Label("Lyrics", systemImage: "text.quote")
                        .font(.headline)
                    Text(lyricsText)
                        .font(.callout)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else if let details {
                VStack(alignment: .leading, spacing: 8) {
                    Label("Lyrics", systemImage: "text.quote")
                        .font(.headline)
                    PhoneInstrumentalLyricsToken(
                        token: details.lyricsDocument?.instrumentalToken
                            ?? LyricsDocument.defaultInstrumentalToken
                    )
                }
            }

            if let notes = details?.notes,
               !notes.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Label("Notes", systemImage: "note.text")
                        .font(.headline)
                    Text(notes)
                        .font(.callout)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct PhoneNowPlayingLyricsView: View {
    @ObservedObject var model: AppModel
    let dismiss: () -> Void

    var body: some View {
        NavigationStack {
            ZStack {
                PhoneLyricsBackdrop(artworkURL: artworkURL)

                VStack(spacing: 0) {
                    if let track = model.playback.nowPlaying {
                        HStack(spacing: 12) {
                            PhoneArtworkImage(
                                artworkURL: artworkURL,
                                placeholderSystemImage: "music.note",
                                size: 44,
                                cornerRadius: 8
                            )
                            .shadow(color: .black.opacity(0.24), radius: 8, y: 4)

                            VStack(alignment: .leading, spacing: 3) {
                                Text(details?.displayTitle ?? track.phoneDisplayTitle)
                                    .font(.headline.weight(.semibold))
                                    .foregroundStyle(.white)
                                    .lineLimit(1)
                                Text(details?.displayArtist ?? track.phoneDisplaySubtitle)
                                    .font(.subheadline)
                                    .foregroundStyle(.white.opacity(0.66))
                                    .lineLimit(1)
                            }
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 24)
                        .padding(.top, 6)
                        .padding(.bottom, 8)
                    }

                    PhoneLyricsContentView(
                        model: model,
                        document: details?.lyricsDocument,
                        fallbackText: details?.lyricsText,
                        isLoading: model.playback.isLoadingDetails
                    )
                    .id(model.playback.nowPlaying?.id)
                }
            }
            .toolbarBackground(.hidden, for: .navigationBar)
            .toolbarColorScheme(.dark, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button(action: dismiss) {
                        Label("Close Lyrics", systemImage: "chevron.down")
                    }
                }
            }
        }
        .preferredColorScheme(.dark)
    }

    private var artworkURL: URL? {
        details?.artworkURL ?? model.playback.nowPlaying?.artworkURL
    }

    private var details: TrackDetails? {
        guard let track = model.playback.nowPlaying else {
            return nil
        }
        if let details = model.playback.details,
           details.identity == track.identity {
            return details
        }
        if let details = model.trackDetail.details,
           details.identity == track.identity {
            return details
        }
        return nil
    }
}

struct PhoneLyricsBackdrop: View {
    let artworkURL: URL?

    var body: some View {
        ZStack {
            Color.black

            if let artworkURL,
               let image = UIImage(contentsOfFile: artworkURL.path) {
                GeometryReader { proxy in
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFill()
                        .frame(width: proxy.size.width, height: proxy.size.height)
                        .clipped()
                        .scaleEffect(1.16)
                        .blur(radius: 46)
                        .saturation(0.82)
                        .opacity(0.58)
                }
            }

            Color.black.opacity(0.52)
        }
        .ignoresSafeArea()
        .accessibilityHidden(true)
    }
}

struct PhoneLyricsContentView: View {
    @ObservedObject var model: AppModel
    let document: LyricsDocument?
    let fallbackText: String?
    let isLoading: Bool

    var body: some View {
        Group {
            if isLoading {
                VStack(spacing: 10) {
                    ProgressView()
                    Text("Loading lyrics…")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let document {
                switch document.content {
                case .timed(let lines):
                    if lines.isEmpty {
                        unavailableLyrics()
                    } else {
                        PhoneTimedLyricsView(model: model, document: document)
                    }
                case .plain(let text):
                    if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        unavailableLyrics()
                    } else {
                        plainLyrics(text)
                    }
                case .instrumental:
                    instrumental(token: document.instrumentalToken)
                }
            } else if let fallbackText,
                      !fallbackText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                plainLyrics(fallbackText)
            } else {
                unavailableLyrics()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func plainLyrics(_ text: String) -> some View {
        ScrollView {
            Text(text)
                .font(.title3.weight(.medium))
                .foregroundStyle(.white.opacity(0.92))
                .lineSpacing(11)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 28)
                .padding(.top, 28)
                .padding(.bottom, 64)
        }
        .scrollIndicators(.hidden)
    }

    private func instrumental(token: String) -> some View {
        lyricsStatus(
            token: token,
            title: "Instrumental",
            message: "This track has no lyrics."
        )
    }

    private func unavailableLyrics() -> some View {
        lyricsStatus(
            token: LyricsDocument.defaultInstrumentalToken,
            title: "No Lyrics",
            message: "Lyrics aren't available for this song."
        )
    }

    private func lyricsStatus(
        token: String,
        title: String,
        message: String
    ) -> some View {
        VStack(spacing: 12) {
            Text(token)
                .font(.system(size: 46, weight: .semibold))
                .foregroundStyle(.white.opacity(0.9))
            Text(title)
                .font(.title2.weight(.bold))
                .foregroundStyle(.white)
            Text(message)
                .font(.callout)
                .foregroundStyle(.white.opacity(0.58))
                .multilineTextAlignment(.center)
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .combine)
    }
}

struct PhoneTimedLyricsView: View {
    @ObservedObject var model: AppModel
    let document: LyricsDocument
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @ScaledMetric(relativeTo: .largeTitle) private var activeFontSize = 31
    @ScaledMetric(relativeTo: .title2) private var inactiveFontSize = 23
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
            GeometryReader { geometry in
                let edgeBreathingRoom = max(96, geometry.size.height * 0.46)

                ZStack(alignment: .bottomTrailing) {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 28) {
                            ForEach(presentationLines) { line in
                                lyricButton(line, isActive: line.id == activeID)
                                    .id(line.id)
                            }
                        }
                        .padding(.horizontal, 28)
                        .padding(.vertical, edgeBreathingRoom)
                    }
                    .scrollIndicators(.hidden)
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
                                .labelStyle(.iconOnly)
                                .font(.system(size: 16, weight: .semibold))
                                .frame(width: 44, height: 44)
                        }
                        .buttonStyle(.plain)
                        .foregroundStyle(.white)
                        .background(.ultraThinMaterial, in: Circle())
                        .overlay {
                            Circle()
                                .stroke(.white.opacity(0.14), lineWidth: 0.5)
                        }
                        .shadow(color: .black.opacity(0.2), radius: 8, y: 3)
                        .padding(18)
                        .accessibilityLabel("Follow Lyrics")
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
    }

    private func lyricButton(
        _ line: TimedLyricsLine,
        isActive: Bool
    ) -> some View {
        let text = line.text.trimmingCharacters(in: .whitespacesAndNewlines)
        let isInstrumental = text.isEmpty

        return Button {
            followsPlayback = true
            Task { await model.seek(toMilliseconds: line.startMS) }
        } label: {
            Text(isInstrumental ? document.instrumentalToken : line.text)
                .font(
                    .system(
                        size: isActive ? activeFontSize : inactiveFontSize,
                        weight: isActive ? .bold : .semibold
                    )
                )
                .foregroundStyle(
                    isActive
                        ? Color.white
                        : Color.white.opacity(0.44)
                )
                .multilineTextAlignment(.leading)
                .lineSpacing(5)
                .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(isInstrumental ? "Instrumental" : line.text)
        .accessibilityValue(isActive ? "Current lyric" : "")
        .accessibilityHint("Seek to \(formatTime(line.startMS))")
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

    private func formatTime(_ milliseconds: Int) -> String {
        let seconds = max(0, milliseconds) / 1_000
        return String(format: "%d:%02d", seconds / 60, seconds % 60)
    }
}

struct PhoneInstrumentalLyricsToken: View {
    let token: String

    var body: some View {
        Text(token)
            .font(.title2)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .center)
            .accessibilityLabel("Instrumental")
    }
}

#endif
