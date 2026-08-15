import Foundation

@MainActor
extension AppModel {
    public func presentCreatePlaylist(addingPickerTrack: Bool = false) {
        playlists.addsPickerTrackAfterCreate = addingPickerTrack && playlists.pickerTrack != nil
        if !playlists.addsPickerTrackAfterCreate {
            playlists.pickerTrack = nil
        }
        playlists.newNameDraft = defaultNewPlaylistName()
        playlists.presentedSheet = .create
    }

    public func cancelCreatePlaylist() {
        dismissPlaylistSheet()
    }

    public func createPlaylist() async {
        let name = playlists.newNameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else {
            operations.status = "Playlist name is empty"
            return
        }

        let pickerTrack = playlists.addsPickerTrackAfterCreate ? playlists.pickerTrack : nil
        await runBusy("Creating \(name)") { [self] in
            let completionStatus: String
            if let pickerTrack {
                let added = try await invoke {
                    try $0.addToPlaylist(name: name, path: pickerTrack.path)
                }
                completionStatus = added
                    ? "Created \(name) and added \(pickerTrack.title)"
                    : "\(pickerTrack.title) is already in \(name)"
            } else {
                try await invoke { try $0.createPlaylist(name: name) }
                completionStatus = "Created \(name)"
            }
            library.scope = .playlist(name)
            playlists.sortMode = .defaultOrder
            await refreshPlaylists()
            await reloadActiveScope(quiet: true)
            operations.status = completionStatus
            dismissPlaylistSheet()
        }
    }

    public func presentPlaylistSettings(_ playlist: PlaylistItem) {
        playlists.settingsOriginalName = playlist.name
        playlists.settingsNameDraft = playlist.name
        playlists.settingsArtworkURL = nil
        playlists.settingsCurrentArtworkURL = playlist.artworkURL
        playlists.pickerTrack = nil
        playlists.addsPickerTrackAfterCreate = false
        playlists.presentedSheet = .settings
    }

    public func cancelPlaylistSettings() {
        dismissPlaylistSheet()
    }

    public func setPlaylistSettingsArtworkURL(_ imageURL: URL) {
        playlists.settingsArtworkURL = imageURL
    }

    public func savePlaylistSettings() async {
        guard let oldName = playlists.settingsOriginalName else {
            operations.status = "Select a playlist first"
            return
        }
        let newName = playlists.settingsNameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !newName.isEmpty else {
            operations.status = "Playlist name is empty"
            return
        }
        let artworkURL = playlists.settingsArtworkURL

        await runBusy("Updating \(oldName)") { [self] in
            let artworkAccessGranted = artworkURL?.startAccessingSecurityScopedResource() ?? false
            defer {
                if artworkAccessGranted {
                    artworkURL?.stopAccessingSecurityScopedResource()
                }
            }
            var currentName = oldName
            var didRename = false
            if newName != oldName {
                try await invoke { try $0.renamePlaylist(oldName: oldName, newName: newName) }
                currentName = newName
                didRename = true
                if library.scope == .playlist(oldName) {
                    library.scope = .playlist(newName)
                }
            }
            if let artworkURL {
                let artworkPlaylistName = currentName
                try await invoke { try $0.setPlaylistArtwork(name: artworkPlaylistName, imageURL: artworkURL) }
            }

            if didRename && artworkURL != nil {
                operations.status = "Updated \(currentName)"
            } else if didRename {
                operations.status = "Renamed playlist"
            } else if artworkURL != nil {
                operations.status = "Updated \(currentName) artwork"
            } else {
                operations.status = "Playlist unchanged"
            }
            playlists.presentedSheet = nil
            clearPlaylistSettingsDraft()
            await refreshPlaylists()
            if library.scope == .playlist(currentName) {
                await reloadActiveScope(quiet: true)
            }
        }
    }

    public func setPlaylistArtwork(_ playlist: PlaylistItem, imageURL: URL) async {
        await setPlaylistArtwork(name: playlist.name, imageURL: imageURL)
    }

    internal func setPlaylistArtwork(name: String, imageURL: URL) async {
        await runBusy("Setting playlist artwork") { [self] in
            let accessGranted = imageURL.startAccessingSecurityScopedResource()
            defer {
                if accessGranted {
                    imageURL.stopAccessingSecurityScopedResource()
                }
            }
            try await invoke { try $0.setPlaylistArtwork(name: name, imageURL: imageURL) }
            operations.status = "Updated \(name) artwork"
            await refreshPlaylists()
        }
    }

    public func addSelectedToPlaylist() async {
        guard let track = library.selectedTrack ?? playback.nowPlaying else {
            operations.status = "Select a track first"
            return
        }
        guard let name = activePlaylistName else {
            operations.status = "Open a playlist first"
            return
        }

        _ = await add(track, toPlaylistNamed: name)
    }

    public func presentPlaylistPicker(for track: TrackItem? = nil) {
        guard let target = track ?? detailTrack else {
            operations.status = "Select or play a track first"
            return
        }
        playlists.pickerTrack = target
        playlists.addsPickerTrackAfterCreate = false
        playlists.presentedSheet = .picker
    }

    public func cancelPlaylistPicker() {
        dismissPlaylistSheet()
    }

    public func dismissPlaylistSheet() {
        if playlists.presentedSheet == .settings {
            clearPlaylistSettingsDraft()
        }
        playlists.presentedSheet = nil
        playlists.pickerTrack = nil
        playlists.addsPickerTrackAfterCreate = false
    }

    public func addPlaylistPickerTrack(to playlist: PlaylistItem) async {
        guard let track = playlists.pickerTrack else {
            operations.status = "Select or play a track first"
            return
        }

        if await add(track, toPlaylistNamed: playlist.name) != nil {
            dismissPlaylistSheet()
        }
    }

    internal func add(_ track: TrackItem, toPlaylistNamed name: String) async -> Bool? {
        var added: Bool?
        await runBusy("Adding to \(name)") { [self] in
            added = try await invoke { try $0.addToPlaylist(name: name, path: track.path) }
            await refreshPlaylists()
            if library.scope == .playlist(name) {
                await reloadActiveScope(quiet: true)
            }
            operations.status = added == true
                ? "Added \(track.title) to \(name)"
                : "\(track.title) is already in \(name)"
        }
        return added
    }

    public func removeSelectedFromActivePlaylist() async {
        guard let track = library.selectedTrack else {
            operations.status = "Select a track first"
            return
        }
        await removeFromActivePlaylist(track)
    }

    public func removeFromActivePlaylist(_ track: TrackItem) async {
        guard let name = activePlaylistName else {
            operations.status = "Select a playlist first"
            return
        }
        await runBusy("Removing from playlist") { [self] in
            try await invoke { try $0.removeFromPlaylist(name: name, path: track.path) }
            operations.status = "Removed \(track.title)"
            await refreshPlaylists()
            await reloadActiveScope(quiet: true)
        }
    }

    public func deleteFromLibrary(_ track: TrackItem) async {
        await runBusy("Deleting \(track.title)") { [self] in
            let summary = try await invoke { try $0.deleteFromLibrary(path: track.path) }
            if library.selectedTrack?.id == track.id {
                library.selectedTrack = nil
                clearDetails()
            }
            invalidateLibraryPresentationCache()
            await refreshPlaylists()
            await reloadActiveScope(quiet: true)
            await refreshPlaybackState()
            if let cleanupError = summary.cleanupError {
                operations.status = "Removed \(track.title) from Library"
                playback.detail = "Managed file cleanup failed: \(cleanupError)"
            } else {
                operations.status = "Deleted \(track.title) from Library"
            }
        }
    }

    public func moveSelectedInActivePlaylist(delta: Int) async {
        guard let name = activePlaylistName else {
            operations.status = "Select a playlist first"
            return
        }
        guard let track = library.selectedTrack else {
            operations.status = "Select a track first"
            return
        }
        await runBusy(nil) { [self] in
            try await invoke { try $0.movePlaylistTrack(name: name, path: track.path, delta: delta) }
            playlists.sortMode = .defaultOrder
            operations.status = "Moved \(track.title)"
            await reloadActiveScope(quiet: true)
        }
    }

    public func sortVisibleTracks(_ sortMode: PlaylistSortMode) async {
        playlists.sortMode = sortMode

        guard let name = activePlaylistName else {
            library.tracks = visibleTracks(from: loadedTracks)
            operations.status = sortMode == .defaultOrder
                ? "\(library.scope.title) default order"
                : "Sorted \(library.scope.title) by \(sortMode.label)"
            return
        }

        await runBusy("Sorting \(name)") { [self] in
            try await invoke { try $0.sortPlaylist(name: name, sort: sortMode.apiValue) }
            operations.status = "Sorted \(name) by \(sortMode.label)"
            await reloadActiveScope(quiet: true)
        }
    }

    public func clearActivePlaylist() async {
        guard let name = activePlaylistName else {
            operations.status = "Select a playlist first"
            return
        }
        await runBusy("Clearing playlist") { [self] in
            try await invoke { try $0.clearPlaylist(name: name) }
            library.selectedTrack = nil
            clearDetails()
            operations.status = "Cleared \(name)"
            await refreshPlaylists()
            await reloadActiveScope(quiet: true)
        }
    }

    public func deleteActivePlaylist() async {
        guard let name = activePlaylistName else {
            operations.status = "Select a playlist first"
            return
        }
        await runBusy("Deleting playlist") { [self] in
            try await invoke { try $0.deletePlaylist(name: name) }
            library.scope = .library
            library.selectedTrack = nil
            clearDetails()
            operations.status = "Deleted \(name)"
            await refreshPlaylists()
            await reloadActiveScope(quiet: true)
        }
    }
}
