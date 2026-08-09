#if os(macOS)
import Foundation
import SwiftUI

struct TrackEditSheet: View {
    @ObservedObject var model: AppModel
    let chooseArtworkFile: () async -> URL?
    let chooseLyricsFile: () async -> URL?

    var body: some View {
        NavigationStack {
            Form {
                Section("Music") {
                    TextField("Title", text: featureBinding(model.trackDetail, \.titleDraft))
                    TextField("Artist", text: featureBinding(model.trackDetail, \.artistDraft))
                    TextField("Album", text: featureBinding(model.trackDetail, \.albumDraft))
                    LabeledContent("Format", value: formatName)
                }

                Section("Artwork") {
                    HStack {
                        Label(selectedArtworkName, systemImage: "photo")
                            .lineLimit(1)
                        Spacer()
                        Button {
                            Task {
                                if let url = await chooseArtworkFile() {
                                    await MainActor.run {
                                        model.setTrackEditArtworkURL(url)
                                    }
                                }
                            }
                        } label: {
                            Label("Choose", systemImage: "folder")
                        }
                    }
                }

                Section("Lyrics") {
                    HStack {
                        Label(selectedLyricsName, systemImage: "text.quote")
                            .lineLimit(1)
                        Spacer()
                        Button {
                            Task {
                                if let url = await chooseLyricsFile() {
                                    await MainActor.run {
                                        model.setTrackEditLyricsURL(url)
                                    }
                                }
                            }
                        } label: {
                            Label("Choose", systemImage: "folder")
                        }
                    }
                }

                Section("Notes") {
                    TextEditor(text: featureBinding(model.trackDetail, \.notesDraft))
                        .font(.callout)
                        .frame(minHeight: 120)
                }
            }
            .navigationTitle("Edit Song")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelTrackEdit()
                    }
                    .disabled(model.trackDetail.isSaving)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        Task { await model.saveTrackEdit() }
                    }
                    .disabled(!canSave)
                }
            }
        }
        .frame(minWidth: 520, idealWidth: 560, maxWidth: 720, minHeight: 560, idealHeight: 620, maxHeight: 760)
        .interactiveDismissDisabled(model.trackDetail.isSaving)
    }

    private var canSave: Bool {
        !model.trackDetail.isSaving
            && model.trackEditChanged
            && !model.trackDetail.titleDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var selectedArtworkName: String {
        model.trackDetail.artworkURL?.lastPathComponent
            ?? model.trackDetail.details?.artworkURL?.lastPathComponent
            ?? "No Artwork"
    }

    private var selectedLyricsName: String {
        model.trackDetail.lyricsURL?.lastPathComponent
            ?? model.trackDetail.details?.lyricsURL?.lastPathComponent
            ?? "No Lyrics"
    }

    private var formatName: String {
        model.trackDetail.details?.formatName?.uppercased()
            ?? model.detailTrack?.formatName?.uppercased()
            ?? "Unknown"
    }

}

struct PlaylistCreateSheet: View {
    @ObservedObject var model: AppModel

    var body: some View {
        NavigationStack {
            Form {
                Section("Playlist") {
                    TextField("Name", text: featureBinding(model.playlists, \.newNameDraft))
                        .onSubmit {
                            Task { await model.createPlaylist() }
                        }
                }
            }
            .navigationTitle("New Playlist")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelCreatePlaylist()
                    }
                    .disabled(model.operations.isBusy)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") {
                        Task { await model.createPlaylist() }
                    }
                    .disabled(!canCreate)
                }
            }
        }
        .frame(minWidth: 380, idealWidth: 420, maxWidth: 520, minHeight: 180, idealHeight: 220, maxHeight: 300)
        .interactiveDismissDisabled(model.operations.isBusy)
    }

    private var canCreate: Bool {
        !model.operations.isBusy && !model.playlists.newNameDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

struct PlaylistSettingsSheet: View {
    @ObservedObject var model: AppModel
    let chooseArtworkFile: () async -> URL?

    var body: some View {
        NavigationStack {
            Form {
                Section("Playlist") {
                    TextField("Name", text: featureBinding(model.playlists, \.settingsNameDraft))
                }

                Section("Cover") {
                    HStack(spacing: 10) {
                        PlaylistArtworkThumbnail(artworkURL: previewArtworkURL)
                            .frame(width: 30, height: 30)
                        Text(artworkName)
                            .lineLimit(1)
                        Spacer()
                        Button {
                            Task {
                                if let imageURL = await chooseArtworkFile() {
                                    await MainActor.run {
                                        model.setPlaylistSettingsArtworkURL(imageURL)
                                    }
                                }
                            }
                        } label: {
                            Label("Choose", systemImage: "folder")
                        }
                    }
                }
            }
            .navigationTitle("Playlist")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelPlaylistSettings()
                    }
                    .disabled(model.operations.isBusy)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        Task { await model.savePlaylistSettings() }
                    }
                    .disabled(!canSave)
                }
            }
        }
        .frame(minWidth: 440, idealWidth: 480, maxWidth: 620, minHeight: 260, idealHeight: 320, maxHeight: 460)
        .interactiveDismissDisabled(model.operations.isBusy)
    }

    private var previewArtworkURL: URL? {
        model.playlists.settingsArtworkURL ?? model.playlists.settingsCurrentArtworkURL
    }

    private var artworkName: String {
        if let artworkURL = model.playlists.settingsArtworkURL {
            return artworkURL.lastPathComponent
        }
        if let artworkURL = model.playlists.settingsCurrentArtworkURL {
            return artworkURL.lastPathComponent
        }
        return "No Cover"
    }

    private var canSave: Bool {
        !model.operations.isBusy
            && model.playlistSettingsChanged
            && !model.playlists.settingsNameDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}
#endif
