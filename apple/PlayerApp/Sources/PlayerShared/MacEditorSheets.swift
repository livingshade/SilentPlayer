#if os(macOS)
import Foundation
import SwiftUI

struct MacPlaylistSheetHost: View {
    @ObservedObject var model: AppModel
    let chooseArtworkFile: () async -> URL?

    @ViewBuilder
    var body: some View {
        switch model.playlists.presentedSheet {
        case .create:
            PlaylistCreateSheet(model: model)
        case .picker:
            PlaylistPickerSheet(model: model)
        case .settings:
            PlaylistSettingsSheet(
                model: model,
                chooseArtworkFile: chooseArtworkFile
            )
        case nil:
            EmptyView()
        }
    }
}

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

struct PlaylistPickerSheet: View {
    @ObservedObject var model: AppModel

    var body: some View {
        NavigationStack {
            List {
                if model.playlists.items.isEmpty {
                    VStack(spacing: 10) {
                        Image(systemName: "music.note.house")
                            .font(.system(size: 36))
                            .foregroundStyle(.secondary)
                        Text("No Playlists")
                            .font(.headline)
                        Text("Create a playlist, then add this song to it.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, minHeight: 220)
                    .listRowSeparator(.hidden)
                } else {
                    ForEach(model.playlists.items) { playlist in
                        Button {
                            Task { await model.addPlaylistPickerTrack(to: playlist) }
                        } label: {
                            HStack(spacing: 10) {
                                PlaylistArtworkThumbnail(artworkURL: playlist.artworkURL)

                                Text(playlist.name)
                                    .lineLimit(1)

                                Spacer(minLength: 0)

                                Text("\(playlist.trackCount) songs")
                                    .font(.caption.monospacedDigit())
                                    .foregroundStyle(.secondary)
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(model.operations.isBusy)
                        .accessibilityLabel("Add to \(playlist.name)")
                        .accessibilityHint("Adds the selected song to this playlist")
                    }
                }
            }
            .navigationTitle("Add to Playlist")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelPlaylistPicker()
                    }
                    .disabled(model.operations.isBusy)
                }
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        model.presentCreatePlaylist(addingPickerTrack: true)
                    } label: {
                        Label("New Playlist", systemImage: "plus")
                    }
                    .disabled(model.operations.isBusy)
                }
            }
        }
        .frame(minWidth: 420, idealWidth: 480, maxWidth: 620, minHeight: 340, idealHeight: 420, maxHeight: 620)
        .interactiveDismissDisabled(model.operations.isBusy)
        .task {
            await model.refreshPlaylists()
        }
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
