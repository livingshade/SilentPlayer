#if os(iOS)
import Foundation
import SwiftUI

struct PhonePlaylistSheetHost: View {
    @ObservedObject var model: AppModel
    let chooseArtwork: () -> Void

    @ViewBuilder
    var body: some View {
        switch model.playlists.presentedSheet {
        case .create:
            PhonePlaylistCreateSheet(model: model)
        case .picker:
            PhonePlaylistPickerSheet(model: model)
        case .settings:
            PhonePlaylistSettingsSheet(model: model, chooseArtwork: chooseArtwork)
        case nil:
            EmptyView()
        }
    }
}

struct PhoneTrackEditSheet: View {
    @ObservedObject var model: AppModel
    let chooseArtwork: () -> Void
    let chooseLyrics: () -> Void

    var body: some View {
        NavigationStack {
            Form {
                Section("Music") {
                    TextField("Title", text: featureBinding(model.trackDetail, \.titleDraft))
                    TextField("Artist", text: featureBinding(model.trackDetail, \.artistDraft))
                    TextField("Album", text: featureBinding(model.trackDetail, \.albumDraft))
                }

                Section("Artwork") {
                    Button {
                        chooseArtwork()
                    } label: {
                        Label(artworkName, systemImage: "photo")
                    }
                }

                Section("Lyrics") {
                    Button {
                        chooseLyrics()
                    } label: {
                        Label(lyricsName, systemImage: "text.quote")
                    }
                }

                Section("Notes") {
                    TextEditor(text: featureBinding(model.trackDetail, \.notesDraft))
                        .frame(minHeight: 140)
                }
            }
            .navigationTitle("Edit Song")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelTrackEdit()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        Task { await model.saveTrackEdit() }
                    }
                    .disabled(!canSave)
                }
            }
        }
        .interactiveDismissDisabled(model.trackDetail.isSaving)
    }

    private var canSave: Bool {
        !model.trackDetail.isSaving
            && model.trackEditChanged
            && !model.trackDetail.titleDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var artworkName: String {
        model.trackDetail.artworkURL?.lastPathComponent
            ?? model.detailDetails?.artworkURL?.lastPathComponent
            ?? "Choose Artwork"
    }

    private var lyricsName: String {
        model.trackDetail.lyricsURL?.lastPathComponent
            ?? model.detailDetails?.lyricsURL?.lastPathComponent
            ?? "Choose Lyrics"
    }
}

struct PhonePlaylistCreateSheet: View {
    @ObservedObject var model: AppModel

    var body: some View {
        NavigationStack {
            Form {
                Section("Playlist") {
                    TextField("Name", text: featureBinding(model.playlists, \.newNameDraft))
                }
            }
            .navigationTitle("New Playlist")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelCreatePlaylist()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") {
                        Task { await model.createPlaylist() }
                    }
                    .disabled(model.playlists.newNameDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
    }
}

struct PhonePlaylistPickerSheet: View {
    @ObservedObject var model: AppModel

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(model.playlists.items) { playlist in
                        Button {
                            Task { await model.addPlaylistPickerTrack(to: playlist) }
                        } label: {
                            HStack(spacing: 12) {
                                PhoneArtworkImage(
                                    artworkURL: playlist.artworkURL,
                                    placeholderSystemImage: "music.note.house",
                                    size: 38,
                                    cornerRadius: 7
                                )

                                VStack(alignment: .leading, spacing: 2) {
                                    Text(playlist.name)
                                        .foregroundStyle(.primary)
                                    Text("\(playlist.trackCount) songs")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                }
            }
            .overlay {
                if model.playlists.items.isEmpty {
                    PhoneEmptyState(
                        title: "No Playlists",
                        message: model.operations.status,
                        systemImage: "music.note.house"
                    )
                }
            }
            .navigationTitle("Add to Playlist")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelPlaylistPicker()
                    }
                }

                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        model.presentCreatePlaylist(addingPickerTrack: true)
                    } label: {
                        Label("New Playlist", systemImage: "plus")
                    }
                }
            }
        }
        .task {
            await model.refreshPlaylists()
        }
    }
}

struct PhonePlaylistSettingsSheet: View {
    @ObservedObject var model: AppModel
    let chooseArtwork: () -> Void

    var body: some View {
        NavigationStack {
            Form {
                Section("Playlist") {
                    TextField("Name", text: featureBinding(model.playlists, \.settingsNameDraft))
                }

                Section("Cover") {
                    Button {
                        chooseArtwork()
                    } label: {
                        Label(artworkName, systemImage: "photo")
                    }
                }
            }
            .navigationTitle("Playlist")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelPlaylistSettings()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        Task { await model.savePlaylistSettings() }
                    }
                    .disabled(!model.playlistSettingsChanged)
                }
            }
        }
    }

    private var artworkName: String {
        model.playlists.settingsArtworkURL?.lastPathComponent
            ?? model.playlists.settingsCurrentArtworkURL?.lastPathComponent
            ?? "Choose Cover"
    }
}

struct PhoneTrackRow: View {
    let track: TrackItem
    let isCurrent: Bool
    let isPlaying: Bool

    var body: some View {
        HStack(spacing: 12) {
            PhoneArtworkImage(
                artworkURL: track.artworkURL,
                placeholderSystemImage: isPlaying ? "speaker.wave.2.fill" : "music.note",
                size: 46,
                cornerRadius: 8
            )

            VStack(alignment: .leading, spacing: 3) {
                Text(track.phoneDisplayTitle)
                    .font(.body.weight(isCurrent ? .semibold : .regular))
                    .fixedSize(horizontal: false, vertical: true)
                Text(track.phoneDisplaySubtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .layoutPriority(1)

            Text(track.durationText)
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
                .fixedSize()
        }
        .padding(.vertical, 4)
    }
}

extension String {
    var phoneCompacted: String {
        PhoneDisplayText.compact(self)
    }
}

extension TrackItem {
    var phoneDisplayTitle: String {
        title.phoneCompacted
    }

    var phoneDisplaySubtitle: String {
        subtitle.phoneCompacted
    }
}

#endif
