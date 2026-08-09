import Foundation

@MainActor
extension AppModel {
    public func importFolder(_ folder: URL) async {
        #if os(macOS)
        startLibraryWorker(.importFolder(folder), status: "Importing \(folder.lastPathComponent)")
        #else
        await runBusy("Importing \(folder.lastPathComponent)") { [self] in
            let accessGranted = folder.startAccessingSecurityScopedResource()
            defer {
                if accessGranted {
                    folder.stopAccessingSecurityScopedResource()
                }
            }
            let summary = try await invoke { try $0.importFolder(folder) }
            invalidateLibraryPresentationCache()
            operations.status = "Imported \(summary.imported), duplicates \(summary.duplicatesSkipped)"
            playback.detail = "Copied \(summary.copied), artwork \(summary.artworkCached), warnings \(summary.metadataWarnings)"
            await reloadActiveScope(quiet: true)
            await refreshPlaylists()
        }
        #endif
    }

    public func importFiles(_ files: [URL]) async {
        let files = files.filter { !$0.path.isEmpty }
        guard !files.isEmpty else {
            operations.status = "No files selected"
            playback.detail = ""
            writeImportDebugLog("importFiles called with no usable file paths")
            return
        }

        writeImportDebugLog(
            "importFiles selected \(files.count): " +
            files.map { $0.path }.joined(separator: " | ")
        )
        await runBusy("Importing \(files.count) files") { [self] in
            let scopedAccess = files.map { url in
                (url, url.startAccessingSecurityScopedResource())
            }
            writeImportDebugLog(
                "security scoped access: " +
                scopedAccess
                    .map { "\($0.0.lastPathComponent)=\($0.1)" }
                    .joined(separator: ", ")
            )
            defer {
                for (url, accessGranted) in scopedAccess where accessGranted {
                    url.stopAccessingSecurityScopedResource()
                }
            }

            let summary = try await invoke { try $0.importFiles(files) }
            invalidateLibraryPresentationCache()
            writeImportDebugLog(
                "importFiles summary imported=\(summary.imported) copied=\(summary.copied) duplicates=\(summary.duplicatesSkipped) warnings=\(summary.metadataWarnings)"
            )
            operations.status = "Imported \(summary.imported), duplicates \(summary.duplicatesSkipped)"
            playback.detail = "Copied \(summary.copied), artwork \(summary.artworkCached), warnings \(summary.metadataWarnings)"
            await reloadActiveScope(quiet: true)
            await refreshPlaylists()
        }
    }

    public func stopLibraryWork() {
        #if os(macOS)
        guard let libraryWorker else {
            return
        }
        libraryWorker.stop()
        self.libraryWorker = nil
        operations.isLibraryWorking = false
        operations.isBusy = false
        operations.libraryProgress = nil
        operations.libraryStatus = "Library task stopped"
        operations.status = "Library task stopped"
        Task {
            await reloadActiveScope(quiet: true)
            await refreshPlaylists()
        }
        #else
        operations.status = "Current library task cannot be interrupted"
        #endif
    }

    public func auditDatabase() async {
        #if os(macOS)
        startLibraryWorker(.audit, status: "Auditing database")
        #else
        await runBusy("Auditing database") { [self] in
            let summary = try await invoke { try $0.auditDatabase() }
            invalidateLibraryPresentationCache()
            operations.status = "Audit finished"
            playback.detail = "Scanned \(summary.tracksScanned), hashes \(summary.hashesUpdated), groups \(summary.duplicateGroups), merged \(summary.tracksMerged), failures \(summary.failures)"
            await reloadActiveScope(quiet: true)
            await refreshPlaylists()
        }
        #endif
    }

    public func showLibrary() async {
        cacheCurrentLibraryPresentationIfNeeded()
        library.scope = .library
        playlists.sortMode = .defaultOrder
        library.query = ""

        if restoreLibraryPresentationFromCache() {
            return
        }
        await reloadActiveScope()
    }

    public func refreshLibrary(quiet: Bool = false) async {
        library.scope = .library
        playlists.sortMode = .defaultOrder
        library.query = ""
        await reloadActiveScope(quiet: quiet)
    }

    public func showFavorites() async {
        cacheCurrentLibraryPresentationIfNeeded()
        library.scope = .favorites
        playlists.sortMode = .defaultOrder
        library.query = ""
        await reloadActiveScope()
    }

    public func showHistory() async {
        cacheCurrentLibraryPresentationIfNeeded()
        library.scope = .history
        playlists.sortMode = .defaultOrder
        library.query = ""
        await reloadActiveScope()
    }

    public func showPlaylist(_ playlist: PlaylistItem) async {
        cacheCurrentLibraryPresentationIfNeeded()
        library.scope = .playlist(playlist.name)
        playlists.sortMode = .defaultOrder
        library.query = ""
        applyLoadedTracks([], preferredSelectedTrackID: nil)
        await reloadActiveScope()
        await refreshPlaylists()
    }

    public func reloadActiveScope(quiet: Bool = false) async {
        await reloadActiveScope(quiet: quiet, preferredSelectedTrackID: nil, forceDetails: false)
    }

    internal func reloadActiveScope(
        quiet: Bool = false,
        preferredSelectedTrackID: String?,
        forceDetails: Bool
    ) async {
        let loadingScope = library.scope
        await runBusy(quiet ? nil : "Loading \(loadingScope.title)") { [self] in
            var loaded: [TrackItem]
            switch loadingScope {
            case .library:
                loaded = try await loadLibraryPages()
            case .favorites:
                loaded = try await invoke { try $0.favorites() }
            case .history:
                loaded = try await invoke { try $0.history() }
            case .playlist(let name):
                loaded = try await invoke { try $0.playlistTracks(name: name) }
            }
            guard library.scope == loadingScope else {
                return
            }
            applyLoadedTracks(
                loaded,
                preferredSelectedTrackID: preferredSelectedTrackID
            )
            if loadingScope == .library {
                isPresentingCompleteLibrary = true
                cacheCurrentLibraryPresentationIfNeeded()
            } else {
                isPresentingCompleteLibrary = false
            }
            if let selectedTrack = library.selectedTrack {
                loadDetails(for: selectedTrack, force: forceDetails)
            } else if let nowPlaying = playback.nowPlaying {
                loadDetails(for: nowPlaying, force: forceDetails)
            }
            operations.status = loaded.isEmpty
                ? "\(loadingScope.title) is empty"
                : "\(loadingScope.title): \(library.tracks.count) songs"
        }
    }

    internal func applyRestoredLibraryScope(_ scope: RestorableLibraryScope) {
        switch scope {
        case .library:
            library.scope = .library
        case .history:
            library.scope = .history
        case .playlist(let id):
            if let playlist = playlists.items.first(where: { $0.id == id }) {
                library.scope = .playlist(playlist.name)
            } else {
                library.scope = .library
            }
        }
        playlists.sortMode = .defaultOrder
        library.query = ""
    }

    public func selectTrack(id: String?) {
        let newSelection = id.flatMap { id in library.tracks.first(where: { $0.id == id }) }
        if let newSelection, library.selectedTrack?.id == newSelection.id {
            loadDetails(for: newSelection)
            return
        }

        library.selectedTrack = newSelection
        if let selectedTrack = library.selectedTrack {
            loadDetails(for: selectedTrack)
        } else if let nowPlaying = playback.nowPlaying {
            loadDetails(for: nowPlaying)
        } else {
            clearDetails()
        }
        cacheCurrentLibraryPresentationIfNeeded()
    }

    public func search() async {
        let trimmed = library.query.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            await reloadActiveScope()
            return
        }

        let searchScope = library.scope
        guard searchScope == .library || activePlaylistName != nil else {
            operations.status = "Search is available in Library and playlists"
            return
        }
        cacheCurrentLibraryPresentationIfNeeded()
        await runBusy("Searching \(searchScope.title)") { [self] in
            let loaded: [TrackItem]
            switch searchScope {
            case .library:
                loaded = try await invoke { try $0.search(trimmed, limit: 200) }
            case .playlist(let name):
                loaded = try await invoke {
                    try $0.searchPlaylist(name: name, query: trimmed, limit: 200)
                }
            case .favorites, .history:
                return
            }
            guard library.scope == searchScope else {
                return
            }
            applyLoadedTracks(loaded, preferredSelectedTrackID: library.selectedTrack?.id)
            isPresentingCompleteLibrary = false
            operations.status = library.tracks.isEmpty
                ? "No songs found in \(searchScope.title)"
                : "\(library.tracks.count) songs found in \(searchScope.title)"
        }
    }
}
