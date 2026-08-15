import Foundation

@MainActor
extension AppModel {
    internal func cacheCurrentLibraryPresentationIfNeeded() {
        guard library.scope == .library, isPresentingCompleteLibrary else {
            return
        }
        libraryPresentationCache = LibraryPresentationCache(
            tracks: loadedTracks,
            selectedTrackID: library.selectedTrack?.id
        )
    }

    internal func restoreLibraryPresentationFromCache() -> Bool {
        guard let cache = libraryPresentationCache else {
            return false
        }

        isPresentingCompleteLibrary = true
        let currentSelectionID = library.selectedTrack.flatMap { selected in
            cache.tracks.contains(where: { $0.id == selected.id }) ? selected.id : nil
        }
        applyLoadedTracks(
            cache.tracks,
            preferredSelectedTrackID: currentSelectionID ?? cache.selectedTrackID
        )
        cacheCurrentLibraryPresentationIfNeeded()

        if let selectedTrack = library.selectedTrack {
            loadDetails(for: selectedTrack)
        } else if let nowPlaying = playback.nowPlaying {
            loadDetails(for: nowPlaying)
        }
        operations.status = cache.tracks.isEmpty
            ? "Library is empty"
            : "Library: \(library.tracks.count) songs"
        return true
    }

    internal func invalidateLibraryPresentationCache() {
        libraryPresentationCache = nil
        isPresentingCompleteLibrary = false
    }

    internal func replaceTrackInLibraryCache(_ updated: TrackItem) {
        guard var cache = libraryPresentationCache else {
            return
        }
        if let index = cache.tracks.firstIndex(where: {
            $0.id == updated.id || $0.path == updated.path
        }) {
            cache.tracks[index] = updated
        } else {
            cache.tracks.append(updated)
        }
        if library.selectedTrack?.id == updated.id || library.selectedTrack?.path == updated.path {
            cache.selectedTrackID = updated.id
        }
        libraryPresentationCache = cache
    }

    internal func applyLoadedTracks(_ loaded: [TrackItem], preferredSelectedTrackID: String?) {
        loadedTracks = loaded

        if let preferredSelectedTrackID,
           let preferred = loaded.first(where: { $0.id == preferredSelectedTrackID }) {
            library.selectedTrack = preferred
        } else if let selectedTrack = library.selectedTrack,
                  let refreshed = loaded.first(where: { $0.id == selectedTrack.id }) {
            library.selectedTrack = refreshed
        } else {
            library.selectedTrack = nil
        }

        if let nowPlaying = playback.nowPlaying,
           let refreshed = loaded.first(where: { $0.id == nowPlaying.id }) {
            playback.nowPlaying = refreshed
        }

        library.tracks = visibleTracks(from: loaded)
    }

    internal func visibleTracks(from tracks: [TrackItem]) -> [TrackItem] {
        sortedTrackItems(tracks, by: playlists.sortMode)
    }

    internal func sortedTrackItems(_ items: [TrackItem], by sortMode: PlaylistSortMode) -> [TrackItem] {
        switch sortMode {
        case .defaultOrder:
            return items
        case .title:
            return items.sorted {
                compareSortKeys(
                    [sortValue($0.title), sortValue($0.artist), sortValue($0.album), $0.path],
                    [sortValue($1.title), sortValue($1.artist), sortValue($1.album), $1.path]
                )
            }
        case .artist:
            return items.sorted {
                compareSortKeys(
                    [sortValue($0.artist), sortValue($0.title), sortValue($0.album), $0.path],
                    [sortValue($1.artist), sortValue($1.title), sortValue($1.album), $1.path]
                )
            }
        case .album:
            return items.sorted {
                compareSortKeys(
                    [sortValue($0.album), sortValue($0.title), sortValue($0.artist), $0.path],
                    [sortValue($1.album), sortValue($1.title), sortValue($1.artist), $1.path]
                )
            }
        case .rating:
            return items.sorted {
                let leftRating = $0.rating ?? -1
                let rightRating = $1.rating ?? -1
                if leftRating != rightRating {
                    return leftRating > rightRating
                }
                return compareSortKeys(
                    [sortValue($0.title), sortValue($0.artist), $0.path],
                    [sortValue($1.title), sortValue($1.artist), $1.path]
                )
            }
        }
    }

    internal func compareSortKeys(_ left: [String], _ right: [String]) -> Bool {
        for (leftValue, rightValue) in zip(left, right) {
            let comparison = leftValue.localizedStandardCompare(rightValue)
            if comparison != .orderedSame {
                return comparison == .orderedAscending
            }
        }
        return false
    }

    internal func sortValue(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "\u{10FFFF}" : trimmed
    }

    internal func replaceTrack(_ updated: TrackItem) {
        if let index = loadedTracks.firstIndex(where: {
            $0.id == updated.id || $0.path == updated.path
        }) {
            loadedTracks[index] = updated
        } else {
            loadedTracks.append(updated)
        }

        if library.selectedTrack?.id == updated.id || library.selectedTrack?.path == updated.path {
            library.selectedTrack = updated
        }
        if playback.nowPlaying?.id == updated.id || playback.nowPlaying?.path == updated.path {
            playback.nowPlaying = updated
        }
        library.tracks = visibleTracks(from: loadedTracks)
        replaceTrackInLibraryCache(updated)
        cacheCurrentLibraryPresentationIfNeeded()
    }

    internal func apply(snapshot: PlaybackSnapshot, fallbackTrack: TrackItem? = nil) {
        let previousTrackID = playback.nowPlaying?.id
        playbackPositionReferenceUptime = ProcessInfo.processInfo.systemUptime
        let nextPlaybackError = snapshot.error ?? ""
        if playback.error != nextPlaybackError {
            playback.error = nextPlaybackError
        }
        if playback.elapsedMS != snapshot.positionMS {
            playback.elapsedMS = snapshot.positionMS
        }
        if playback.isPlaying != snapshot.isPlaying {
            playback.isPlaying = snapshot.isPlaying
        }
        if playback.isAudioInterrupted != snapshot.interruptionActive {
            playback.isAudioInterrupted = snapshot.interruptionActive
        }
        resumeAfterAudioInterruption = snapshot.resumeAfterInterruption
        if playback.playbackMode != snapshot.playbackMode {
            playback.playbackMode = snapshot.playbackMode
        }
        if playback.repeatMode != snapshot.repeatMode {
            playback.repeatMode = snapshot.repeatMode
        }
        if playback.isShuffleEnabled != snapshot.shuffleEnabled {
            playback.isShuffleEnabled = snapshot.shuffleEnabled
        }
        if playback.queueCount != snapshot.queueLen {
            playback.queueCount = snapshot.queueLen
        }
        if snapshot.queueLen == 0, !playback.queue.isEmpty {
            playback.queue = []
        }
        if playback.queuePosition != snapshot.queuePosition {
            playback.queuePosition = snapshot.queuePosition
        }

        if let track = snapshot.currentTrack ?? fallbackTrack {
            let trackChanged = playback.nowPlaying != track
            if trackChanged {
                playback.nowPlaying = track
                if !loadedTracks.isEmpty {
                    let visible = visibleTracks(from: loadedTracks)
                    if library.tracks != visible {
                        library.tracks = visible
                    }
                }
                cacheCurrentLibraryPresentationIfNeeded()
            }
            if previousTrackID != track.id {
                let shouldFollowNowPlaying = library.selectedTrack == nil || library.selectedTrack?.id == previousTrackID
                if shouldFollowNowPlaying {
                    library.selectedTrack = loadedTracks.first(where: { $0.id == track.id }) ?? track
                }
            }
            if detailTrack?.id == track.id {
                if detailsTrackID == track.id,
                   let details = trackDetail.details,
                   details.identity == track.identity {
                    playbackDetailsTask?.cancel()
                    playbackDetailsTask = nil
                    playback.details = details
                    playbackDetailsTrackID = track.id
                    playback.isLoadingDetails = false
                } else if previousTrackID != track.id
                    || (trackDetail.details == nil && !trackDetail.isLoading) {
                    loadDetails(for: track)
                }
            } else if previousTrackID != track.id
                || playbackDetailsTrackID != track.id
                || playback.details == nil {
                loadPlaybackDetails(for: track)
            }
        } else if !snapshot.isPlaying {
            if playback.nowPlaying != nil {
                playback.nowPlaying = nil
            }
            clearPlaybackDetails()
            if let selectedTrack = library.selectedTrack {
                loadDetails(for: selectedTrack)
            } else {
                clearDetails()
            }
            if playback.elapsedMS != 0 {
                playback.elapsedMS = 0
            }
        }

        if let gainDB = snapshot.gainDB {
            let detail = String(
                format: "Normalize gain %@ dB",
                String(format: "%+.1f", gainDB)
            )
            if playback.detail != detail {
                playback.detail = detail
            }
        } else if let loudnessStatus = snapshot.loudnessStatus {
            if playback.detail != loudnessStatus {
                playback.detail = loudnessStatus
            }
        }

        if let error = snapshot.error, !error.isEmpty {
            if operations.status != "Playback error" {
                operations.status = "Playback error"
            }
            if playback.error != error {
                playback.error = error
            }
        }

        if playback.nowPlaying == nil {
            if previousTrackID != nil {
                playbackSystemIntegration?.playbackDidStop()
            }
            stopPlaybackTimer()
        } else if PlaybackPollingPolicy.shouldPoll(
            hasNowPlayingItem: true,
            isPlaying: playback.isPlaying
        ) {
            startPlaybackTimer()
        } else {
            stopPlaybackTimer()
        }
    }

    internal func publishPlaybackPositionDiscontinuity(
        previousTrackID: String?,
        previousPositionMS: Int,
        force: Bool = false
    ) {
        guard
            let previousTrackID,
            playback.nowPlaying?.id == previousTrackID,
            playback.elapsedMS != previousPositionMS,
            force || playback.elapsedMS < previousPositionMS
        else {
            return
        }
        playbackSystemIntegration?.playbackPositionDidChange()
    }

    internal func loadDetails(for track: TrackItem, force: Bool = false) {
        if !force {
            if loadingDetailsTrackID == track.id {
                return
            }
            if detailsTrackID == track.id && trackDetail.details != nil {
                return
            }
        }

        detailsTask?.cancel()
        if detailsTrackID != track.id {
            trackDetail.details = TrackDetails.placeholder(for: track)
            detailsTrackID = track.id
        }
        loadingDetailsTrackID = track.id
        trackDetail.isLoading = true

        detailsTask = Task { [weak self] in
            guard let self else {
                return
            }
            do {
                let details = try await self.invoke { try $0.trackDetails(path: track.path) }
                guard !Task.isCancelled else {
                    return
                }
                if self.detailTrack?.id == track.id {
                    self.trackDetail.details = details
                    self.detailsTrackID = track.id
                    self.loadingDetailsTrackID = nil
                    self.trackDetail.isLoading = false
                }
                if self.playback.nowPlaying?.id == track.id {
                    self.playbackDetailsTask?.cancel()
                    self.playbackDetailsTask = nil
                    self.playback.details = details
                    self.playbackDetailsTrackID = track.id
                    self.playback.isLoadingDetails = false
                }
            } catch {
                guard !Task.isCancelled else {
                    return
                }
                if self.detailTrack?.id == track.id {
                    self.loadingDetailsTrackID = nil
                    self.trackDetail.isLoading = false
                    self.playback.detail = "Details unavailable: \(error.localizedDescription)"
                }
                if self.playback.nowPlaying?.id == track.id {
                    self.playback.isLoadingDetails = false
                }
            }
        }
    }

    internal func loadPlaybackDetails(for track: TrackItem, force: Bool = false) {
        if !force,
           playbackDetailsTrackID == track.id,
           playback.details != nil {
            return
        }

        playbackDetailsTask?.cancel()
        if playbackDetailsTrackID != track.id {
            playback.details = TrackDetails.placeholder(for: track)
            playbackDetailsTrackID = track.id
        }
        playback.isLoadingDetails = true

        playbackDetailsTask = Task { [weak self] in
            guard let self else {
                return
            }
            do {
                let details = try await self.invoke { try $0.trackDetails(path: track.path) }
                guard !Task.isCancelled else {
                    return
                }
                if self.playback.nowPlaying?.id == track.id {
                    self.playback.details = details
                    self.playbackDetailsTrackID = track.id
                    self.playbackDetailsTask = nil
                    self.playback.isLoadingDetails = false
                }
            } catch {
                guard !Task.isCancelled else {
                    return
                }
                if self.playback.nowPlaying?.id == track.id {
                    self.playbackDetailsTask = nil
                    self.playback.isLoadingDetails = false
                    self.playback.detail = "Now Playing details unavailable: \(error.localizedDescription)"
                }
            }
        }
    }

    internal func clearPlaybackDetails() {
        playbackDetailsTask?.cancel()
        playbackDetailsTask = nil
        playbackDetailsTrackID = nil
        playback.details = nil
        playback.isLoadingDetails = false
    }

    internal func clearDetails() {
        detailsTask?.cancel()
        detailsTask = nil
        detailsTrackID = nil
        loadingDetailsTrackID = nil
        trackDetail.details = nil
        resetTrackEditDrafts()
        trackDetail.isLoading = false
        if playback.nowPlaying == nil {
            clearPlaybackDetails()
        }
    }

    internal func resetTrackEditDrafts() {
        trackEditTarget = nil
        trackDetail.titleDraft = ""
        trackDetail.artistDraft = ""
        trackDetail.albumDraft = ""
        trackDetail.notesDraft = ""
        trackDetail.artworkURL = nil
        trackDetail.lyricsURL = nil
        trackDetail.isSaving = false
    }

    internal func startPlaybackTimer() {
        guard playbackTimer == nil else {
            return
        }
        let timer = Timer.scheduledTimer(
            withTimeInterval: PlaybackPollingPolicy.timerInterval,
            repeats: true
        ) { [weak self] _ in
            Task { @MainActor in
                await self?.pollPlayback()
            }
        }
        timer.tolerance = PlaybackPollingPolicy.timerTolerance
        playbackTimer = timer
    }

    internal func stopPlaybackTimer() {
        playbackTimer?.invalidate()
        playbackTimer = nil
    }

    internal func normalizedDraft(_ value: String?) -> String {
        value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }

    internal func matchingDetails(for track: TrackItem) -> TrackDetails? {
        if let details = trackDetail.details,
           details.identity == track.identity {
            return details
        }
        if let details = playback.details,
           details.identity == track.identity {
            return details
        }
        return nil
    }

    internal func defaultNewPlaylistName() -> String {
        let baseName = "New Playlist"
        let existingNames = Set(playlists.items.map(\.name))
        if !existingNames.contains(baseName) {
            return baseName
        }
        var index = 2
        while existingNames.contains("\(baseName) \(index)") {
            index += 1
        }
        return "\(baseName) \(index)"
    }

    internal func clearPlaylistSettingsDraft() {
        playlists.settingsOriginalName = nil
        playlists.settingsNameDraft = ""
        playlists.settingsArtworkURL = nil
        playlists.settingsCurrentArtworkURL = nil
    }

    internal func runBusy(_ busyStatus: String?, operation: () async throws -> Void) async {
        if let busyStatus {
            operations.status = busyStatus
        }
        operations.isBusy = true
        defer { operations.isBusy = false }

        do {
            try await operation()
        } catch {
            report(error)
        }
    }

    internal func loadLibraryPages() async throws -> [TrackItem] {
        let pageSize = 100
        var loaded: [TrackItem] = []
        var offset = 0
        var expectedTotal = 0
        operations.libraryProgress = 0
        operations.libraryStatus = "Loading Library"
        defer {
            operations.libraryProgress = nil
            operations.libraryStatus = ""
        }

        while true {
            let requestedOffset = offset
            let page = try await invoke {
                try $0.libraryPage(offset: requestedOffset, limit: pageSize)
            }
            guard page.offset == requestedOffset else {
                throw RustPlayerError.callFailed(
                    "Library page offset mismatch: expected \(requestedOffset), received \(page.offset)"
                )
            }
            expectedTotal = page.total
            loaded.append(contentsOf: page.tracks)
            let completed = min(loaded.count, expectedTotal)
            operations.libraryProgress = expectedTotal == 0
                ? 1
                : min(Double(completed) / Double(expectedTotal), 1)
            operations.libraryStatus = "Loading Library \(completed) / \(expectedTotal)"
            operations.status = operations.libraryStatus

            if completed >= expectedTotal {
                return loaded
            }
            guard !page.tracks.isEmpty else {
                throw RustPlayerError.callFailed(
                    "Library loading stopped at \(completed) of \(expectedTotal) items"
                )
            }
            offset = requestedOffset + page.tracks.count
            await Task.yield()
        }
    }

    internal var serviceUnavailableError: RustPlayerError {
        .startupFailed(startupError ?? "Player service is unavailable")
    }

    internal func requireClient() throws -> RustPlayerClient {
        guard let client else {
            throw serviceUnavailableError
        }
        return client
    }

    internal func invoke<T: Sendable>(
        priority: TaskPriority = .userInitiated,
        _ operation: @escaping @Sendable (RustPlayerClient) throws -> T
    ) async throws -> T {
        let client = try requireClient()
        return try await Task.detached(priority: priority) {
            try operation(client)
        }.value
    }

    internal func report(_ error: Error) {
        operations.status = "Error"
        playback.error = error.localizedDescription
        writeImportDebugLog("error: \(error.localizedDescription)")
    }

    internal func writeImportDebugLog(_ message: String) {
        #if os(iOS)
        let timestamp = ISO8601DateFormatter().string(from: Date())
        let line = "[\(timestamp)] \(message)\n"
        guard let data = line.data(using: .utf8) else {
            return
        }
        let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let logURL = documents.appendingPathComponent("import-debug.log")
        if FileManager.default.fileExists(atPath: logURL.path),
           let handle = try? FileHandle(forWritingTo: logURL) {
            defer { try? handle.close() }
            _ = try? handle.seekToEnd()
            try? handle.write(contentsOf: data)
        } else {
            try? data.write(to: logURL)
        }
        #else
        _ = message
        #endif
    }

    internal func formatTime(_ milliseconds: Int) -> String {
        let totalSeconds = max(0, milliseconds / 1000)
        return "\(totalSeconds / 60):\(String(format: "%02d", totalSeconds % 60))"
    }
}
