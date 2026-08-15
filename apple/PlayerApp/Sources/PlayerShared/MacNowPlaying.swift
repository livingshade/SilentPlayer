#if os(macOS)
import Foundation
import SwiftUI

extension ContentView {
    internal func nowPlayingPanel(for track: TrackItem, layout: DetailPaneLayout) -> some View {
        HStack(alignment: .top, spacing: 18) {
            ArtworkViewport(
                artworkURL: model.trackDetail.details?.artworkURL,
                size: layout.artworkSize
            )
            .frame(width: layout.artworkSize)

            ScrollView(.vertical) {
                VStack(alignment: .leading, spacing: 10) {
                    HStack(alignment: .top, spacing: 12) {
                        VStack(alignment: .leading, spacing: 5) {
                            Text(track.title)
                                .font(.title3.weight(.semibold))
                                .lineLimit(2)
                            Text(track.subtitle)
                                .font(.callout)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                            HStack(spacing: 12) {
                                Label(track.durationText, systemImage: "clock")
                                Label(track.gainText, systemImage: "speaker.wave.2")
                                playbackStatusLabel(for: track)
                            }
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }

                        Spacer(minLength: 8)

                        VStack(alignment: .trailing, spacing: 8) {
                            ratingPicker(for: track)
                                .frame(maxWidth: 140, alignment: .trailing)
                            trackActionsMenu(for: track)
                        }
                    }

                    secondaryContentPanels
                    fileDetailsPanel
                }
                .padding(.vertical, 1)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .frame(height: layout.detailPanelHeight, alignment: .topLeading)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.55))
    }

    internal func expandedNowPlaying(for track: TrackItem) -> some View {
        let details = playbackDetails(for: track)
        let artworkURL = details?.artworkURL ?? track.artworkURL
        return ZStack {
            NowPlayingBackdrop(artworkURL: artworkURL)

            GeometryReader { proxy in
                let leftWidth = min(max(proxy.size.width * 0.41, 290), 430)
                let notesHeight = min(max(proxy.size.height * 0.23, 118), 168)

                HStack(alignment: .top, spacing: 22) {
                    ViewThatFits(in: .vertical) {
                        expandedDetailColumn(
                            for: track,
                            details: details,
                            artworkSize: min(210, proxy.size.height * 0.29)
                        )
                        expandedDetailColumn(
                            for: track,
                            details: details,
                            artworkSize: 132
                        )
                        expandedDetailColumn(
                            for: track,
                            details: details,
                            artworkSize: 92
                        )
                    }
                    .frame(width: leftWidth)
                    .frame(maxHeight: .infinity, alignment: .top)

                    Divider()

                    VStack(alignment: .leading, spacing: 12) {
                        HStack {
                            Label("Lyrics", systemImage: "text.quote")
                                .font(.headline)
                            Spacer()
                            if let format = details?.lyricsDocument?.format {
                                Text(format == .lrc ? "Synced" : "Plain Text")
                                    .font(.caption2.weight(.medium))
                                    .foregroundStyle(.secondary)
                            }
                            Button {
                                dismissExpandedNowPlaying()
                            } label: {
                                Label("Close Now Playing", systemImage: "xmark")
                                    .labelStyle(.iconOnly)
                                    .frame(width: 24, height: 24)
                            }
                            .buttonStyle(.bordered)
                            .keyboardShortcut(.cancelAction)
                            .help("Close Now Playing")
                        }

                        NowPlayingLyricsView(
                            model: model,
                            document: details?.lyricsDocument,
                            fallbackText: details?.lyricsText,
                            isLoading: model.playback.isLoadingDetails
                        )
                        .id(track.id)
                        .layoutPriority(1)

                        Divider()

                        expandedNotes(for: track, details: details)
                            .frame(height: notesHeight, alignment: .top)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
                .padding(.horizontal, 22)
                .padding(.vertical, 18)
            }
        }
    }

    internal func expandedDetailColumn(
        for track: TrackItem,
        details: TrackDetails?,
        artworkSize: CGFloat
    ) -> some View {
        VStack(spacing: 10) {
            ArtworkViewport(
                artworkURL: details?.artworkURL ?? track.artworkURL,
                size: artworkSize
            )

            VStack(spacing: 4) {
                Text(track.title)
                    .font(.title2.weight(.semibold))
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                Text(track.artist)
                    .font(.title3.weight(.medium))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                Text(track.album)
                    .font(.callout)
                    .foregroundStyle(.tertiary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity)

            HStack(spacing: 12) {
                ratingPicker(for: track)
                    .frame(maxWidth: 150)
                Spacer(minLength: 4)
                trackActionsMenu(for: track)
            }

            expandedTrackFacts(for: track, details: details)
            expandedPlaybackHistory(details: details)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }

    internal func expandedPlaybackProgress(for track: TrackItem) -> some View {
        VStack(spacing: 7) {
            TimelineView(.periodic(from: .now, by: model.playback.isPlaying ? 0.2 : 1)) { _ in
                let positionMS = model.estimatedPlaybackPositionMS()
                HStack {
                    Text(playbackTimestamp(positionMS))
                    Spacer()
                    Text(track.durationText)
                }
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
            }

            Slider(
                value: seekBinding,
                in: 0...1,
                onEditingChanged: handleSeekEditingChanged
            )
            .controlSize(.large)
            .disabled(track.durationMS == nil)
        }
    }

    internal var expandedPlaybackControls: some View {
        HStack(spacing: 22) {
            Menu {
                ForEach(PlaybackMode.allCases) { mode in
                    Button {
                        Task { await model.setPlaybackMode(mode) }
                    } label: {
                        Label(
                            mode.label,
                            systemImage: model.playback.playbackMode == mode ? "checkmark" : mode.systemImage
                        )
                    }
                }
            } label: {
                Label(
                    model.playback.playbackMode.label,
                    systemImage: model.playback.playbackMode.systemImage
                )
                    .labelStyle(.iconOnly)
                    .foregroundStyle(
                        model.playback.playbackMode == .sequential
                            ? Color.secondary
                            : Color.accentColor
                    )
            }
            .menuStyle(.borderlessButton)
            .help("Playback order: \(model.playback.playbackMode.label)")

            Button {
                Task { await model.previousTrack() }
            } label: {
                Label("Previous", systemImage: "backward.fill")
                    .labelStyle(.iconOnly)
                    .font(.title3)
            }
            .buttonStyle(.borderless)
            .help("Previous")

            Button {
                Task { await model.pauseOrResume() }
            } label: {
                Label(
                    model.playback.isPlaying ? "Pause" : "Play",
                    systemImage: model.playback.isPlaying ? "pause.fill" : "play.fill"
                )
                .labelStyle(.iconOnly)
                .font(.title2)
                .frame(width: 30, height: 30)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .clipShape(Circle())
            .help(model.playback.isPlaying ? "Pause" : "Play")

            Button {
                Task { await model.nextTrack() }
            } label: {
                Label("Next", systemImage: "forward.fill")
                    .labelStyle(.iconOnly)
                    .font(.title3)
            }
            .buttonStyle(.borderless)
            .help("Next")

            Button {
                isQueuePresented = true
            } label: {
                Label(model.queueStatusText, systemImage: "music.note.list")
                    .labelStyle(.iconOnly)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.borderless)
            .help(model.queueStatusText)
        }
    }

    internal func expandedTrackFacts(
        for track: TrackItem,
        details: TrackDetails?
    ) -> some View {
        VStack(alignment: .leading, spacing: 9) {
            Label("Track Details", systemImage: "info.circle")
                .font(.headline)

            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 7) {
                GridRow {
                    Label(track.durationText, systemImage: "clock")
                    Label(details?.formatName ?? track.formatName ?? "Unknown format", systemImage: "waveform")
                }
                GridRow {
                    Label(details?.qualityProfile ?? track.qualityProfile ?? "Quality not set", systemImage: "hifispeaker")
                    Label(track.gainText, systemImage: "speaker.wave.2")
                }
                GridRow {
                    Label(track.ratingText, systemImage: track.rating == nil ? "star" : "star.fill")
                    Label(model.playback.isPlaying ? "Playing" : "Paused", systemImage: model.playback.isPlaying ? "waveform" : "pause.circle")
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .lineLimit(1)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .textBackgroundColor).opacity(0.65))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    internal func expandedPlaybackHistory(details: TrackDetails?) -> some View {
        VStack(alignment: .leading, spacing: 9) {
            Label("Listening History", systemImage: "clock.arrow.circlepath")
                .font(.headline)

            Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 7) {
                GridRow {
                    LabeledContent("Plays", value: "\(details?.playCount ?? 0)")
                    LabeledContent("Sessions", value: "\(details?.playbackSessionCount ?? 0)")
                }
                GridRow {
                    LabeledContent(
                        "Last Played",
                        value: playbackDateText(details?.lastPlayedAtUnixSeconds)
                    )
                    LabeledContent(
                        "Last Completed",
                        value: playbackDateText(details?.lastCompletedAtUnixSeconds)
                    )
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .textBackgroundColor).opacity(0.65))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    internal func expandedNotes(
        for track: TrackItem,
        details: TrackDetails?
    ) -> some View {
        let notes = details?.notes?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return VStack(alignment: .leading, spacing: 8) {
            HStack {
                Label("Notes", systemImage: "note.text")
                    .font(.headline)
                Spacer()
                Button {
                    model.presentTrackEdit(for: track)
                } label: {
                    Label("Edit Notes", systemImage: "pencil")
                }
                .buttonStyle(.borderless)
                .disabled(model.playback.isLoadingDetails)
            }

            Text(notes.isEmpty ? "No notes" : notes)
                .font(.callout)
                .foregroundStyle(notes.isEmpty ? Color.secondary : Color.primary)
                .lineLimit(4)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .textBackgroundColor).opacity(0.65))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    internal func playbackDateText(_ unixSeconds: Int64?) -> String {
        guard let unixSeconds else {
            return "Never"
        }
        return Date(timeIntervalSince1970: TimeInterval(unixSeconds))
            .formatted(date: .abbreviated, time: .shortened)
    }

    internal func playbackTimestamp(_ milliseconds: Int) -> String {
        let totalSeconds = max(0, milliseconds / 1_000)
        return "\(totalSeconds / 60):\(String(format: "%02d", totalSeconds % 60))"
    }

    internal func playbackDetails(for track: TrackItem) -> TrackDetails? {
        guard let details = model.playback.details,
              details.identity == track.identity else {
            return nil
        }
        return details
    }

    internal func ratingPicker(for track: TrackItem) -> some View {
        Picker(
            selection: Binding(
                get: { model.detailTrack?.rating ?? 0 },
                set: { value in
                    Task { await model.setRating(value == 0 ? nil : value) }
                }
            )
        ) {
            Text("Unrated").tag(0)
            ForEach(1...10, id: \.self) { value in
                Text("\(value)/10").tag(value)
            }
        } label: {
            Label(track.ratingText, systemImage: track.rating == nil ? "star" : "star.fill")
        }
        .pickerStyle(.menu)
        .help("Set rating")
    }

    internal func trackActionsMenu(for track: TrackItem) -> some View {
        Menu {
            Button {
                Task { await model.addToQueue(track) }
            } label: {
                Label("Add to Queue", systemImage: "text.line.last.and.arrowtriangle.forward")
            }

            Button {
                model.presentPlaylistPicker(for: track)
            } label: {
                Label("Add to Playlist…", systemImage: "text.badge.plus")
            }

            Divider()

            Button {
                setTrackCover(for: track)
            } label: {
                Label("Set Track Cover", systemImage: "photo")
            }

            Button {
                setAlbumCover(for: track)
            } label: {
                Label("Set Album Cover", systemImage: "rectangle.stack.badge.plus")
            }
            .disabled(!track.hasAlbumIdentity)

            Divider()

            Button {
                model.presentTrackEdit()
            } label: {
                Label("Edit Song…", systemImage: "pencil")
            }
            .disabled(model.trackDetail.isLoading || model.detailTrack == nil)

            Button {
                materialize(track)
            } label: {
                Label("Export Song…", systemImage: "square.and.arrow.down")
            }
            .disabled(model.detailTrack == nil)
        } label: {
            Label("Track Actions", systemImage: "ellipsis.circle")
                .labelStyle(.iconOnly)
        }
        .menuStyle(.borderlessButton)
        .help("Track actions")
    }

}
#endif
