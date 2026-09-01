#if os(iOS)
import Foundation
import SwiftUI

struct PhoneTrackDetailView: View {
    @ObservedObject var model: AppModel
    let track: TrackItem
    let requestAddToPlaylist: (TrackItem) -> Void
    let requestTrackCover: (TrackItem) -> Void
    let requestAlbumCover: (TrackItem) -> Void
    let exportTrack: (TrackItem) -> Void

    var body: some View {
        let currentTrack = displayedTrack
        let currentDetails = details

        List {
            Section {
                PhoneTrackDetailHeader(
                    track: currentTrack,
                    details: currentDetails,
                    isPlaying: model.playback.nowPlaying?.id == currentTrack.id && model.playback.isPlaying
                )
                .frame(maxWidth: .infinity)
                .listRowInsets(EdgeInsets(top: 20, leading: 16, bottom: 20, trailing: 16))
            }

            Section("Playback") {
                Button {
                    model.selectTrack(id: currentTrack.id)
                    Task { await model.play(currentTrack) }
                } label: {
                    Label("Play", systemImage: "play.fill")
                }

                LabeledContent("Position", value: model.playback.nowPlaying?.id == currentTrack.id ? model.playbackTimeText : currentTrack.durationText)
                LabeledContent("Loudness", value: currentTrack.gainText)
                LabeledContent("Queue", value: queueStatus(for: currentTrack))
            }

            Section("Song") {
                Picker("Rating", selection: ratingBinding) {
                    Text("Unrated").tag(0)
                    ForEach(1...10, id: \.self) { value in
                        Text("\(value)/10").tag(value)
                    }
                }

                if let currentDetails {
                    LabeledContent("Format", value: optionalValue(currentDetails.formatName ?? currentTrack.formatName))
                    LabeledContent("Quality", value: optionalValue(currentDetails.qualityProfile ?? currentTrack.qualityProfile))
                }
            }

            Section("Metadata") {
                LabeledContent("Title", value: currentDetails?.displayTitle ?? currentTrack.title)
                LabeledContent("Artist", value: currentDetails?.displayArtist ?? currentTrack.artist)
                LabeledContent("Album", value: currentDetails?.displayAlbum ?? currentTrack.album)

                if let currentDetails, hasOriginalMetadata(currentDetails) {
                    DisclosureGroup("Original Metadata") {
                        LabeledContent("Title", value: currentDetails.originalTitle)
                        LabeledContent("Artist", value: currentDetails.originalArtist)
                        LabeledContent("Album", value: currentDetails.originalAlbum)
                    }
                }
            }

            if let lyrics = currentDetails?.lyricsText?.trimmingCharacters(in: .whitespacesAndNewlines),
               !lyrics.isEmpty {
                Section("Lyrics") {
                    Text(lyrics)
                        .font(.body)
                        .textSelection(.enabled)
                }
            } else if let currentDetails {
                Section("Lyrics") {
                    PhoneInstrumentalLyricsToken(
                        token: currentDetails.lyricsDocument?.instrumentalToken
                            ?? LyricsDocument.defaultInstrumentalToken
                    )
                }
            }

            if let notes = currentDetails?.notes?.trimmingCharacters(in: .whitespacesAndNewlines),
               !notes.isEmpty {
                Section("Notes") {
                    Text(notes)
                        .font(.body)
                        .textSelection(.enabled)
                }
            }

            if let currentDetails {
                let importantDiagnostics = currentDetails.diagnostics.filter { $0.severity != .info }
                if !importantDiagnostics.isEmpty {
                    Section("Needs Attention") {
                        ForEach(importantDiagnostics) { diagnostic in
                            PhoneDiagnosticRow(diagnostic: diagnostic)
                        }
                    }
                }
            }

            Section("Actions") {
                Button {
                    Task { await model.playNext(currentTrack) }
                } label: {
                    Label("Play Next", systemImage: "text.line.first.and.arrowtriangle.forward")
                }

                Button {
                    Task { await model.addToQueue(currentTrack) }
                } label: {
                    Label("Add to Queue", systemImage: "text.badge.plus")
                }

                Button {
                    requestAddToPlaylist(currentTrack)
                } label: {
                    Label("Add to Playlist", systemImage: "text.badge.plus")
                }

                Button {
                    model.selectTrack(id: currentTrack.id)
                    model.presentTrackEdit()
                } label: {
                    Label("Edit Song", systemImage: "pencil")
                }

                Button {
                    requestTrackCover(currentTrack)
                } label: {
                    Label("Set Track Cover", systemImage: "photo")
                }

                Button {
                    requestAlbumCover(currentTrack)
                } label: {
                    Label("Set Album Cover", systemImage: "rectangle.stack.badge.plus")
                }
                .disabled(!currentTrack.hasAlbumIdentity)

                Button {
                    exportTrack(currentTrack)
                } label: {
                    Label("Export Song", systemImage: "square.and.arrow.up")
                }
            }
        }
        .navigationTitle(currentTrack.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItemGroup(placement: .bottomBar) {
                Button {
                    Task { await model.previousTrack() }
                } label: {
                    Label("Previous", systemImage: "backward.fill")
                }
                .disabled(model.playback.nowPlaying == nil)

                Spacer()

                Button {
                    model.selectTrack(id: currentTrack.id)
                    Task { await model.play(currentTrack) }
                } label: {
                    Label("Play", systemImage: "play.fill")
                }

                Spacer()

                Button {
                    Task { await model.nextTrack() }
                } label: {
                    Label("Next", systemImage: "forward.fill")
                }
                .disabled(model.playback.nowPlaying == nil)
            }
        }
        .task {
            model.selectTrack(id: track.id)
        }
    }

    private func queueStatus(for track: TrackItem) -> String {
        guard let index = model.playback.queue.firstIndex(where: { $0.id == track.id }) else {
            return "Not queued"
        }
        if model.playback.queuePosition == index {
            return model.queueStatusText
        }
        return "Queued at \(index + 1) of \(model.playback.queue.count)"
    }

    private var displayedTrack: TrackItem {
        if let detailTrack = model.detailTrack,
           detailTrack.id == track.id {
            return detailTrack
        }
        return track
    }

    private var details: TrackDetails? {
        guard let details = model.trackDetail.details,
              details.identity == displayedTrack.identity else {
            return nil
        }
        return details
    }

    private var ratingBinding: Binding<Int> {
        Binding(
            get: { details?.rating ?? displayedTrack.rating ?? 0 },
            set: { value in
                model.selectTrack(id: displayedTrack.id)
                Task { await model.setRating(value == 0 ? nil : value) }
            }
        )
    }

    private func hasOriginalMetadata(_ details: TrackDetails) -> Bool {
        details.originalTitle != details.displayTitle
            || details.originalArtist != details.displayArtist
            || details.originalAlbum != details.displayAlbum
    }

    private func optionalValue(_ value: String?) -> String {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return "Not set"
        }
        return value
    }
}

struct PhoneTrackDetailHeader: View {
    let track: TrackItem
    let details: TrackDetails?
    let isPlaying: Bool

    var body: some View {
        VStack(spacing: 12) {
            PhoneArtworkImage(
                artworkURL: details?.artworkURL ?? track.artworkURL,
                placeholderSystemImage: isPlaying ? "speaker.wave.2.fill" : "music.note",
                size: 220,
                cornerRadius: 14
            )

            VStack(spacing: 4) {
                Text(details?.displayTitle ?? track.title)
                    .font(.title2.weight(.semibold))
                    .multilineTextAlignment(.center)
                    .lineLimit(3)
                Text(track.subtitle)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
                Text("\(track.durationText) · \(track.ratingText)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

struct PhoneDiagnosticRow: View {
    let diagnostic: TrackDiagnostic

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 3) {
                Text(diagnostic.title)
                    .font(.body)
                Text(diagnostic.detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        } icon: {
            Image(systemName: systemImage)
                .foregroundStyle(color)
        }
    }

    private var systemImage: String {
        switch diagnostic.severity {
        case .error:
            return "xmark.octagon.fill"
        case .warning:
            return "exclamationmark.triangle.fill"
        case .info:
            return "info.circle"
        }
    }

    private var color: Color {
        switch diagnostic.severity {
        case .error:
            return .red
        case .warning:
            return .orange
        case .info:
            return .secondary
        }
    }
}

#endif
