import Foundation

@MainActor
extension AppModel {
    public func setSelectedFavorite(_ enabled: Bool = true) async {
        guard let track = library.selectedTrack ?? playback.nowPlaying else {
            operations.status = "Select a track first"
            return
        }

        await runBusy(enabled ? "Adding favorite" : "Removing favorite") { [self] in
            try await invoke { try $0.setFavorite(path: track.path, enabled: enabled) }
            operations.status = enabled ? "Favorited \(track.title)" : "Removed favorite"
            if library.scope == .favorites {
                await reloadActiveScope(quiet: true)
            }
        }
    }

    public func setRating(_ rating: Int?) async {
        guard let track = detailTrack else {
            operations.status = "Select or play a track first"
            return
        }
        if let rating, !(1...10).contains(rating) {
            operations.status = "Rating must be between 1 and 10"
            return
        }

        await runBusy("Updating rating") { [self] in
            let updated = try await invoke { try $0.setTrackRating(path: track.path, rating: rating) }
            replaceTrack(updated)
            operations.status = rating.map { "Rated \($0)/10" } ?? "Cleared rating"
            loadDetails(for: updated, force: true)
        }
    }

    public func setTrackArtwork(for track: TrackItem, imageURL: URL) async {
        await runBusy("Setting track cover") { [self] in
            let accessGranted = imageURL.startAccessingSecurityScopedResource()
            defer {
                if accessGranted {
                    imageURL.stopAccessingSecurityScopedResource()
                }
            }
            let updated = try await invoke { try $0.setTrackArtwork(path: track.path, imageURL: imageURL) }
            replaceTrack(updated)
            operations.status = "Saved track cover"
            await reloadActiveScope(
                quiet: true,
                preferredSelectedTrackID: updated.id,
                forceDetails: true
            )
            await refreshPlaylists()
        }
    }

    public func setTrackArtwork(_ imageURL: URL) async {
        guard let track = detailTrack else {
            operations.status = "Select or play a track first"
            return
        }
        await setTrackArtwork(for: track, imageURL: imageURL)
    }

    public func setAlbumArtwork(for track: TrackItem, imageURL: URL) async {
        guard track.hasAlbumIdentity else {
            operations.status = "Album metadata is required before setting an album cover"
            return
        }

        await runBusy("Setting album cover") { [self] in
            let accessGranted = imageURL.startAccessingSecurityScopedResource()
            defer {
                if accessGranted {
                    imageURL.stopAccessingSecurityScopedResource()
                }
            }
            let summary = try await invoke { try $0.setAlbumArtwork(path: track.path, imageURL: imageURL) }
            invalidateLibraryPresentationCache()
            operations.status = summary.tracksUpdated == 0
                ? "No tracks matched this album"
                : "Updated album cover for \(summary.tracksUpdated) tracks"
            await reloadActiveScope(
                quiet: true,
                preferredSelectedTrackID: track.id,
                forceDetails: true
            )
            await refreshPlaylists()
        }
    }

    public func setAlbumArtwork(_ imageURL: URL) async {
        guard let track = detailTrack else {
            operations.status = "Select or play a track first"
            return
        }
        await setAlbumArtwork(for: track, imageURL: imageURL)
    }

    public func presentTrackEdit(for requestedTrack: TrackItem? = nil) {
        guard let track = requestedTrack ?? detailTrack else {
            operations.status = "Select or play a track first"
            return
        }
        let details = matchingDetails(for: track)
        trackEditTarget = track
        trackDetail.titleDraft = details?.displayTitle ?? track.title
        trackDetail.artistDraft = details?.displayArtist ?? track.artist
        trackDetail.albumDraft = details?.displayAlbum ?? track.album
        trackDetail.notesDraft = details?.notes ?? ""
        trackDetail.artworkURL = nil
        trackDetail.lyricsURL = nil
        trackDetail.isEditPresented = true
    }

    public func cancelTrackEdit() {
        trackDetail.isEditPresented = false
        resetTrackEditDrafts()
    }

    public func setTrackEditArtworkURL(_ url: URL) {
        trackDetail.artworkURL = url
    }

    public func setTrackEditLyricsURL(_ url: URL) {
        trackDetail.lyricsURL = url
    }

    public func saveTrackEdit() async {
        guard let track = trackEditTarget ?? detailTrack else {
            operations.status = "Select or play a track first"
            return
        }
        let title = trackDetail.titleDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else {
            operations.status = "Title cannot be empty"
            return
        }

        let edit = TrackEdit(
            title: title,
            artist: trackDetail.artistDraft.trimmingCharacters(in: .whitespacesAndNewlines),
            album: trackDetail.albumDraft.trimmingCharacters(in: .whitespacesAndNewlines),
            notes: trackDetail.notesDraft,
            artworkPath: trackDetail.artworkURL?.path,
            lyricsPath: trackDetail.lyricsURL?.path
        )

        trackDetail.isSaving = true
        await runBusy("Saving song") { [self] in
            let artworkAccessGranted = trackDetail.artworkURL?.startAccessingSecurityScopedResource() ?? false
            let lyricsAccessGranted = trackDetail.lyricsURL?.startAccessingSecurityScopedResource() ?? false
            defer {
                if artworkAccessGranted {
                    trackDetail.artworkURL?.stopAccessingSecurityScopedResource()
                }
                if lyricsAccessGranted {
                    trackDetail.lyricsURL?.stopAccessingSecurityScopedResource()
                }
            }
            let updated = try await invoke { try $0.editTrack(path: track.path, edit: edit) }
            replaceTrack(updated)
            operations.status = "Saved song"
            trackDetail.isEditPresented = false
            resetTrackEditDrafts()
            await reloadActiveScope(
                quiet: true,
                preferredSelectedTrackID: updated.id,
                forceDetails: true
            )
        }
        trackDetail.isSaving = false
    }

    public func materializeSelected(to destinationURL: URL) async {
        guard let track = detailTrack else {
            operations.status = "Select or play a track first"
            return
        }

        await runBusy("Exporting song") { [self] in
            let materialized = try await invoke {
                try $0.exportTrack(path: track.path, destinationURL: destinationURL)
            }
            replaceTrack(materialized)
            operations.status = "Exported \(materialized.title)"
            await reloadActiveScope(
                quiet: true,
                preferredSelectedTrackID: materialized.id,
                forceDetails: true
            )
        }
    }
}
