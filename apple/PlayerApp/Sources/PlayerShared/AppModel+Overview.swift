import Foundation

@MainActor
extension AppModel {
    public var dbPath: String {
        client?.dbURL.path ?? ""
    }

    public var mediaRootPath: String {
        client?.mediaRootURL.path ?? ""
    }

    public var repoPath: String {
        client?.repoRoot.path ?? ""
    }

    public var isPaused: Bool {
        playback.nowPlaying != nil && !playback.isPlaying
    }

    public var playbackProgress: Double? {
        guard let durationMS = playback.nowPlaying?.durationMS, durationMS > 0 else {
            return nil
        }
        return min(max(Double(playback.elapsedMS) / Double(durationMS), 0), 1)
    }

    public var playbackTimeText: String {
        "\(formatTime(playback.elapsedMS)) / \(playback.nowPlaying?.durationText ?? "--:--")"
    }

    public func estimatedPlaybackPositionMS(
        atUptime uptime: TimeInterval = ProcessInfo.processInfo.systemUptime
    ) -> Int {
        PlaybackPresentationClock.positionMS(
            basePositionMS: playback.elapsedMS,
            baseUptime: playbackPositionReferenceUptime,
            currentUptime: uptime,
            isPlaying: playback.isPlaying,
            durationMS: playback.nowPlaying?.durationMS
        )
    }

    public var normalizeText: String {
        if let gainDB = playback.nowPlaying?.gainDB {
            return String(format: "Normalize %@ dB", String(format: "%+.1f", gainDB))
        }
        return playback.nowPlaying?.loudnessStatus ?? "Normalize pending"
    }

    public var queueStatusText: String {
        guard playback.queueCount > 0 else {
            return "Queue empty"
        }
        if let queuePosition = playback.queuePosition {
            return "Queue \(queuePosition + 1) / \(playback.queueCount)"
        }
        return "Queue \(playback.queueCount)"
    }

    public var activePlaylistName: String? {
        if case .playlist(let name) = library.scope {
            return name
        }
        return nil
    }

    public var restorableLibraryScope: RestorableLibraryScope {
        switch library.scope {
        case .library, .favorites:
            return .library
        case .history:
            return .history
        case .playlist(let name):
            guard let playlist = playlists.items.first(where: { $0.name == name }) else {
                return .library
            }
            return .playlist(playlist.id)
        }
    }

    public var detailTrack: TrackItem? {
        library.selectedTrack ?? playback.nowPlaying
    }

    public var detailDetails: TrackDetails? {
        guard let track = detailTrack else {
            return nil
        }
        return matchingDetails(for: track)
    }

    public var trackEditChanged: Bool {
        guard let track = trackEditTarget ?? detailTrack else {
            return false
        }
        let details = matchingDetails(for: track)
        return trackDetail.titleDraft != (details?.displayTitle ?? track.title)
            || trackDetail.artistDraft != (details?.displayArtist ?? track.artist)
            || trackDetail.albumDraft != (details?.displayAlbum ?? track.album)
            || trackDetail.notesDraft != (details?.notes ?? "")
            || trackDetail.artworkURL != nil
            || trackDetail.lyricsURL != nil
    }

    public var playlistSettingsChanged: Bool {
        guard let originalName = playlists.settingsOriginalName else {
            return false
        }
        return normalizedDraft(playlists.settingsNameDraft) != originalName
            || playlists.settingsArtworkURL != nil
    }

    public var isShowingInitialDetailsLoad: Bool {
        trackDetail.isLoading && trackDetail.details == nil
    }

    public func bootstrap(
        restoring restorationScope: RestorableLibraryScope? = nil,
        preferredSelectedTrackID: String? = nil
    ) async {
        guard client != nil else {
            operations.status = "Player unavailable"
            playback.error = startupError ?? "Unable to start the player service"
            return
        }
        guard !hasBootstrapped else {
            return
        }
        hasBootstrapped = true
        await refreshPlaylists()
        if let restorationScope {
            applyRestoredLibraryScope(restorationScope)
        }
        await reloadActiveScope(
            preferredSelectedTrackID: preferredSelectedTrackID,
            forceDetails: false
        )
        await refreshPlaybackState()
    }
}
