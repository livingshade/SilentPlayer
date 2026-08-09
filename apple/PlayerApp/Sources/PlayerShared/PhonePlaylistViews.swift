#if os(iOS)
import Foundation
import SwiftUI

struct PhonePlaylistDetailView: View {
    @ObservedObject var model: AppModel
    let playlist: PlaylistItem
    let confirmLibraryDeletion: (TrackItem) -> Void
    @State private var isLoadingPlaylist = true

    var body: some View {
        Group {
            if isLoadingPlaylist {
                VStack(spacing: 14) {
                    ProgressView()
                        .controlSize(.large)
                    Text("Loading \(playlist.name.phoneCompacted)")
                        .font(.callout.weight(.medium))
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                playlistContent
            }
        }
        .navigationTitle(playlist.name.phoneCompacted)
        .navigationBarTitleDisplayMode(.inline)
        .searchable(
            text: playlistSearchBinding,
            prompt: "Search songs in \(playlist.name.phoneCompacted)"
        )
        .onSubmit(of: .search) {
            Task { await model.search() }
        }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    model.presentPlaylistSettings(playlist)
                } label: {
                    Label("Edit Playlist", systemImage: "ellipsis.circle")
                }
            }
        }
        .task(id: playlist.id) {
            isLoadingPlaylist = model.activePlaylistName != playlist.name
            if isLoadingPlaylist {
                await model.showPlaylist(playlist)
            }
            isLoadingPlaylist = false
        }
    }

    private var playlistContent: some View {
        List {
            VStack(spacing: 18) {
                PhoneArtworkImage(
                    artworkURL: playlist.artworkURL,
                    placeholderSystemImage: "music.note.house",
                    size: 176,
                    cornerRadius: 18
                )
                .shadow(color: .black.opacity(0.12), radius: 14, y: 8)

                VStack(spacing: 4) {
                    Text(playlist.name.phoneCompacted)
                        .font(.title2.weight(.bold))
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                    Text("\(playlist.trackCount) songs")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }

                HStack(spacing: 12) {
                    Button {
                        Task {
                            await model.playPlaylist(playlist, shuffled: false)
                        }
                    } label: {
                        Label("Play", systemImage: "play.fill")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)

                    Button {
                        Task {
                            await model.playPlaylist(playlist, shuffled: true)
                        }
                    } label: {
                        Label("Shuffle", systemImage: "shuffle")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                }
                .controlSize(.large)
                .disabled(playlist.trackCount == 0 || model.operations.isBusy)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 12)
            .listRowInsets(EdgeInsets(top: 8, leading: 20, bottom: 16, trailing: 20))
            .listRowBackground(Color.clear)
            .listRowSeparator(.hidden)

            Section("Songs") {
                ForEach(model.library.tracks) { track in
                    let isCurrent = model.playback.nowPlaying?.id == track.id
                    Button {
                        model.selectTrack(id: track.id)
                        Task {
                            await model.playPlaylist(
                                playlist,
                                startingAt: track,
                                shuffled: false
                            )
                        }
                    } label: {
                        PhoneTrackRow(
                            track: track,
                            isCurrent: isCurrent,
                            isPlaying: isCurrent && model.playback.isPlaying
                        )
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Play \(track.phoneDisplayTitle)")
                    .accessibilityHint("Starts this track and queues the playlist")
                    .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                        Button(role: .destructive) {
                            Task { await model.removeFromActivePlaylist(track) }
                        } label: {
                            Label("Remove", systemImage: "minus.circle")
                        }
                    }
                    .contextMenu {
                        Button {
                            Task {
                                await model.playPlaylist(
                                    playlist,
                                    startingAt: track,
                                    shuffled: false
                                )
                            }
                        } label: {
                            Label("Play from Here", systemImage: "play.fill")
                        }

                        Button {
                            Task {
                                await model.playPlaylist(
                                    playlist,
                                    startingAt: track,
                                    shuffled: true
                                )
                            }
                        } label: {
                            Label("Shuffle from Here", systemImage: "shuffle")
                        }

                        Button {
                            Task { await model.playNext(track) }
                        } label: {
                            Label("Play Next", systemImage: "text.line.first.and.arrowtriangle.forward")
                        }

                        Button {
                            Task { await model.addToQueue(track) }
                        } label: {
                            Label("Add to Queue", systemImage: "text.badge.plus")
                        }

                        Divider()

                        Button(role: .destructive) {
                            Task { await model.removeFromActivePlaylist(track) }
                        } label: {
                            Label("Remove from Playlist", systemImage: "minus.circle")
                        }

                        Button(role: .destructive) {
                            confirmLibraryDeletion(track)
                        } label: {
                            Label("Delete from Library…", systemImage: "trash")
                        }
                    }
                }

                if model.library.tracks.isEmpty, !model.operations.isBusy {
                    Text(
                        model.library.query.isEmpty
                            ? "This playlist is empty."
                            : "No songs match “\(model.library.query)”."
                    )
                        .foregroundStyle(.secondary)
                }
            }
        }
        .listStyle(.plain)
    }

    private var playlistSearchBinding: Binding<String> {
        Binding(
            get: { model.library.query },
            set: { newValue in
                let clearedSearch = !model.library.query.isEmpty && newValue.isEmpty
                model.library.query = newValue
                if clearedSearch, model.activePlaylistName == playlist.name {
                    Task { await model.reloadActiveScope() }
                }
            }
        )
    }
}

struct PhoneTrackActionPanel: View {
    @ObservedObject var model: AppModel
    let track: TrackItem
    let requestAddToPlaylist: () -> Void
    let requestTrackCover: () -> Void
    let requestAlbumCover: () -> Void
    let exportTrack: () -> Void

    var body: some View {
        VStack(spacing: 12) {
            Picker("Rating", selection: ratingBinding) {
                Text("Unrated").tag(0)
                ForEach(1...10, id: \.self) { value in
                    Text("\(value)/10").tag(value)
                }
            }
            .pickerStyle(.menu)

            Grid(horizontalSpacing: 12, verticalSpacing: 12) {
                GridRow {
                    Button {
                        Task { await model.playNext(track) }
                    } label: {
                        Label("Play Next", systemImage: "text.line.first.and.arrowtriangle.forward")
                    }

                    Button {
                        Task { await model.addToQueue(track) }
                    } label: {
                        Label("Queue", systemImage: "text.badge.plus")
                    }
                }

                GridRow {
                    Button {
                        requestAddToPlaylist()
                    } label: {
                        Label("Playlist", systemImage: "music.note.list")
                    }

                    Button {
                        model.selectTrack(id: track.id)
                        model.presentTrackEdit()
                    } label: {
                        Label("Edit Song", systemImage: "pencil")
                    }

                    Button {
                        requestTrackCover()
                    } label: {
                        Label("Track Cover", systemImage: "photo")
                    }
                }

                GridRow {
                    Button {
                        requestAlbumCover()
                    } label: {
                        Label("Album Cover", systemImage: "rectangle.stack.badge.plus")
                    }
                    .disabled(!track.hasAlbumIdentity)

                    Button {
                        exportTrack()
                    } label: {
                        Label("Export", systemImage: "square.and.arrow.up")
                    }
                }
            }
            .buttonStyle(.bordered)
        }
        .padding()
        .background(Color(.secondarySystemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
    }

    private var ratingBinding: Binding<Int> {
        Binding(
            get: {
                if model.detailTrack?.id == track.id {
                    return model.detailTrack?.rating ?? 0
                }
                return track.rating ?? 0
            },
            set: { value in
                model.selectTrack(id: track.id)
                Task { await model.setRating(value == 0 ? nil : value) }
            }
        )
    }

}

#endif
