#if os(macOS)
import Foundation
import SwiftUI

extension ContentView {
    internal func handleSeekEditingChanged(_ editing: Bool) {
        if editing {
            if pendingSeekProgress == nil {
                pendingSeekProgress = model.playbackProgress ?? 0
            }
            return
        }
        guard let progress = pendingSeekProgress else {
            return
        }
        Task {
            await model.seek(toProgress: progress)
            if pendingSeekProgress == progress {
                pendingSeekProgress = nil
            }
        }
    }

    internal var emptyIcon: String {
        switch model.library.scope {
        case .library:
            return "music.note.list"
        case .favorites:
            return "heart"
        case .history:
            return "clock"
        case .playlist:
            return "music.note.house"
        }
    }

    internal func playlistSortButton(_ sortMode: PlaylistSortMode) -> some View {
        Button {
            Task { await model.sortVisibleTracks(sortMode) }
        } label: {
            Label(
                sortMode.label,
                systemImage: model.playlists.sortMode == sortMode ? "checkmark" : sortMode.systemImage
            )
        }
    }

    internal func playbackStatusLabel(for track: TrackItem) -> some View {
        Group {
            if model.playback.nowPlaying?.id == track.id && model.playback.isPlaying {
                Label("Playing", systemImage: "waveform")
                    .foregroundStyle(Color.green)
            } else if model.playback.nowPlaying?.id == track.id {
                Label("Paused", systemImage: "pause.circle")
                    .foregroundStyle(.secondary)
            } else {
                Label("Selected", systemImage: "info.circle")
                    .foregroundStyle(.secondary)
            }
        }
    }

    internal func trackRow(for track: TrackItem) -> some View {
        let isCurrent = model.playback.nowPlaying?.id == track.id
        return TrackRow(track: track, isCurrent: isCurrent, isPlaying: isCurrent && model.playback.isPlaying)
            .tag(track.id)
            .contentShape(Rectangle())
            .onTapGesture(count: 2) {
                playTrackFromRow(track)
            }
            .onTapGesture(count: 1) {
                scheduleTrackSelection(track)
            }
            .contextMenu {
                trackContextMenu(for: track)
            }
    }

    internal func scheduleTrackSelection(_ track: TrackItem) {
        pendingSingleClick?.cancel()
        let work = DispatchWorkItem {
            model.selectTrack(id: track.id)
            persistPresentation()
        }
        pendingSingleClick = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.18, execute: work)
    }

    internal func selectTrackImmediately(_ track: TrackItem) {
        pendingSingleClick?.cancel()
        pendingSingleClick = nil
        model.selectTrack(id: track.id)
        persistPresentation()
    }

    internal func playTrackFromRow(_ track: TrackItem) {
        selectTrackImmediately(track)
        Task { await model.play(track) }
    }

    @ViewBuilder
    internal func trackContextMenu(for track: TrackItem) -> some View {
        Button {
            playTrackFromRow(track)
        } label: {
            Label("Play", systemImage: "play.fill")
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

        Button {
            selectTrackImmediately(track)
            Task { await model.addSelectedToPlaylist() }
        } label: {
            Label("Add to Playlist", systemImage: "text.badge.plus")
        }

        Divider()

        Button {
            selectTrackImmediately(track)
            model.presentTrackEdit()
        } label: {
            Label("Edit Song", systemImage: "pencil")
        }

        Button {
            selectTrackImmediately(track)
            setTrackCover(for: track)
        } label: {
            Label("Set Track Cover", systemImage: "photo")
        }

        Button {
            selectTrackImmediately(track)
            setAlbumCover(for: track)
        } label: {
            Label("Set Album Cover", systemImage: "rectangle.stack.badge.plus")
        }
        .disabled(!track.hasAlbumIdentity)

        Button {
            selectTrackImmediately(track)
            materialize(track)
        } label: {
            Label("Export Song", systemImage: "square.and.arrow.down")
        }

        if model.activePlaylistName != nil {
            Divider()

            Button {
                selectTrackImmediately(track)
                Task { await model.moveSelectedInActivePlaylist(delta: -1) }
            } label: {
                Label("Move Up", systemImage: "arrow.up")
            }

            Button {
                selectTrackImmediately(track)
                Task { await model.moveSelectedInActivePlaylist(delta: 1) }
            } label: {
                Label("Move Down", systemImage: "arrow.down")
            }

            Button(role: .destructive) {
                selectTrackImmediately(track)
                Task { await model.removeSelectedFromActivePlaylist() }
            } label: {
                Label("Remove from Playlist", systemImage: "minus.circle")
            }
        }

        Divider()

        Button(role: .destructive) {
            pendingLibraryDeletion = track
            isLibraryDeletionConfirmationPresented = true
        } label: {
            Label("Delete from Library…", systemImage: "trash")
        }
    }

    internal func scopeButton(
        _ title: String,
        icon: String,
        selected: Bool,
        action: @escaping () async -> Void
    ) -> some View {
        Button {
            Task {
                await action()
                persistPresentation()
            }
        } label: {
            HStack {
                Label(title, systemImage: icon)
                Spacer()
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .contentShape(Rectangle())
            .background(selected ? Color.accentColor.opacity(0.14) : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
    }

    internal func playlistButton(_ playlist: PlaylistItem) -> some View {
        let selected = model.library.scope == .playlist(playlist.name)
        return Button {
            Task {
                await model.showPlaylist(playlist)
                persistPresentation()
            }
        } label: {
            HStack(spacing: 8) {
                PlaylistArtworkThumbnail(artworkURL: playlist.artworkURL)
                Text(playlist.name)
                    .lineLimit(1)
                Spacer()
                Text("\(playlist.trackCount)")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .contentShape(Rectangle())
            .background(selected ? Color.accentColor.opacity(0.14) : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button {
                model.presentPlaylistSettings(playlist)
            } label: {
                Label("Rename...", systemImage: "pencil")
            }

            Button {
                Task {
                    if let imageURL = await chooseArtworkFile() {
                        await model.setPlaylistArtwork(playlist, imageURL: imageURL)
                    }
                }
            } label: {
                Label("Set Cover...", systemImage: "photo")
            }
        }
    }

    internal func materialize(_ track: TrackItem) {
        Task {
            if let destination = await chooseExportFile(track) {
                await model.materializeSelected(to: destination)
            }
        }
    }

    internal func setTrackCover(for track: TrackItem) {
        Task {
            if let imageURL = await chooseArtworkFile() {
                await model.setTrackArtwork(for: track, imageURL: imageURL)
            }
        }
    }

    internal func setAlbumCover(for track: TrackItem) {
        Task {
            if let imageURL = await chooseArtworkFile() {
                await model.setAlbumArtwork(for: track, imageURL: imageURL)
            }
        }
    }
}
#endif
