import Foundation

@MainActor
extension AppModel {
    public func playSelected() async {
        guard let track = library.selectedTrack else {
            operations.status = "Select a track first"
            return
        }
        await play(track)
    }

    public func playEntireLibrary() async {
        guard !playback.isAudioInterrupted else {
            operations.status = "Wait for the audio interruption to end"
            return
        }
        await runBusy(nil) { [self] in
            let previousTrackID = playback.nowPlaying?.id
            let previousPositionMS = playback.elapsedMS
            try playbackSystemIntegration?.prepareForPlayback()
            let snapshot = try await invoke { try $0.playLibrary() }
            library.selectedTrack = snapshot.currentTrack
            apply(snapshot: snapshot, fallbackTrack: snapshot.currentTrack)
            await refreshQueue()
            publishPlaybackPositionDiscontinuity(
                previousTrackID: previousTrackID,
                previousPositionMS: previousPositionMS,
                force: true
            )
            operations.status = "Playing all Library"
        }
    }

    public func playAllVisible() async {
        guard let firstTrack = library.tracks.first else {
            operations.status = "\(library.scope.title) is empty"
            return
        }
        await play(firstTrack)
    }

    public func playPlaylist(
        _ playlist: PlaylistItem,
        startingAt track: TrackItem? = nil,
        shuffled: Bool
    ) async {
        guard !playback.isAudioInterrupted else {
            operations.status = "Wait for the audio interruption to end"
            return
        }
        guard playlist.trackCount > 0 else {
            operations.status = "\(playlist.name) is empty"
            return
        }
        await runBusy(nil) { [self] in
            let previousTrackID = playback.nowPlaying?.id
            let previousPositionMS = playback.elapsedMS
            try playbackSystemIntegration?.prepareForPlayback()
            let snapshot = try await invoke {
                try $0.playPlaylist(
                    name: playlist.name,
                    startPath: track?.path,
                    shuffle: shuffled
                )
            }
            library.selectedTrack = snapshot.currentTrack
            apply(snapshot: snapshot, fallbackTrack: track)
            await refreshQueue()
            await refreshPlaylists()
            publishPlaybackPositionDiscontinuity(
                previousTrackID: previousTrackID,
                previousPositionMS: previousPositionMS,
                force: true
            )
            operations.status = shuffled
                ? "Shuffling \(playlist.name)"
                : "Playing \(playlist.name)"
        }
    }

    public func play(_ track: TrackItem) async {
        guard !playback.isAudioInterrupted else {
            operations.status = "Wait for the audio interruption to end"
            return
        }
        await runBusy(nil) { [self] in
            let previousTrackID = playback.nowPlaying?.id
            let previousPositionMS = playback.elapsedMS
            try playbackSystemIntegration?.prepareForPlayback()
            let queuePaths = library.tracks.map(\.path)
            let paths = queuePaths.contains(track.path) ? queuePaths : [track.path]
            let snapshot = try await invoke { try $0.playQueue(paths: paths, startPath: track.path) }
            library.selectedTrack = track
            apply(snapshot: snapshot, fallbackTrack: track)
            await refreshQueue()
            publishPlaybackPositionDiscontinuity(
                previousTrackID: previousTrackID,
                previousPositionMS: previousPositionMS,
                force: true
            )
            operations.status = "Playing \(track.title)"
        }
    }

    public func pauseOrResume() async {
        guard playback.nowPlaying != nil else {
            await playSelected()
            return
        }
        guard playback.isPlaying || !playback.isAudioInterrupted else {
            operations.status = "Wait for the audio interruption to end"
            return
        }

        do {
            let snapshot: PlaybackSnapshot
            if playback.isPlaying {
                snapshot = try await invoke { try $0.pause() }
                operations.status = "Paused"
            } else {
                try playbackSystemIntegration?.prepareForPlayback()
                snapshot = try await invoke { try $0.resume() }
                operations.status = "Playing \(snapshot.currentTrack?.title ?? playback.nowPlaying?.title ?? "")"
            }
            apply(snapshot: snapshot)
        } catch {
            report(error)
        }
    }

    public func stopPlayback() async {
        do {
            let snapshot = try await invoke { try $0.stop() }
            apply(snapshot: snapshot)
            playback.queue = []
            playbackSystemIntegration?.playbackDidStop()
            operations.status = "Stopped"
        } catch {
            report(error)
        }
    }

    public func playNext(_ track: TrackItem) async {
        do {
            let snapshot = try await invoke { try $0.playNext(path: track.path) }
            apply(snapshot: snapshot)
            await refreshQueue()
            operations.status = "\(track.title) will play next"
        } catch {
            report(error)
        }
    }

    public func addToQueue(_ track: TrackItem) async {
        do {
            let previousCount = playback.queueCount
            let snapshot = try await invoke { try $0.addToQueue(path: track.path) }
            apply(snapshot: snapshot)
            await refreshQueue()
            operations.status = snapshot.queueLen == previousCount
                ? "\(track.title) is already in the queue"
                : "Added \(track.title) to queue"
        } catch {
            report(error)
        }
    }

    public func moveQueueItem(from: Int, to: Int) async {
        guard playback.queue.indices.contains(from),
              playback.queue.indices.contains(to),
              from != to else {
            return
        }
        do {
            let snapshot = try await invoke { try $0.moveQueueItem(from: from, to: to) }
            apply(snapshot: snapshot)
            await refreshQueue()
        } catch {
            report(error)
        }
    }

    public func playQueueItem(at index: Int) async {
        guard playback.queue.indices.contains(index) else {
            return
        }
        guard !playback.isAudioInterrupted else {
            operations.status = "Wait for the audio interruption to end"
            return
        }
        let requestedTrack = playback.queue[index]
        do {
            let previousTrackID = playback.nowPlaying?.id
            let previousPositionMS = playback.elapsedMS
            try playbackSystemIntegration?.prepareForPlayback()
            let snapshot = try await invoke { try $0.playQueueItem(at: index) }
            library.selectedTrack = snapshot.currentTrack
            apply(snapshot: snapshot, fallbackTrack: requestedTrack)
            await refreshQueue()
            publishPlaybackPositionDiscontinuity(
                previousTrackID: previousTrackID,
                previousPositionMS: previousPositionMS,
                force: true
            )
            operations.status = "Playing \(snapshot.currentTrack?.title ?? requestedTrack.title)"
        } catch {
            report(error)
        }
    }

    public func removeQueueItem(at index: Int) async {
        guard playback.queue.indices.contains(index) else {
            return
        }
        let removedTitle = playback.queue[index].title
        do {
            let snapshot = try await invoke { try $0.removeQueueItem(at: index) }
            apply(snapshot: snapshot)
            await refreshQueue()
            operations.status = "Removed \(removedTitle) from queue"
        } catch {
            report(error)
        }
    }

    public func clearPlaybackQueue() async {
        do {
            let snapshot = try await invoke { try $0.clearQueue() }
            apply(snapshot: snapshot)
            playback.queue = []
            playbackSystemIntegration?.playbackDidStop()
            operations.status = "Queue cleared"
        } catch {
            report(error)
        }
    }

    public func nextTrack() async {
        do {
            let previousTrackID = playback.nowPlaying?.id
            let previousPositionMS = playback.elapsedMS
            if playback.isPlaying {
                try playbackSystemIntegration?.prepareForPlayback()
            }
            let snapshot = try await invoke { try $0.next() }
            apply(snapshot: snapshot)
            publishPlaybackPositionDiscontinuity(
                previousTrackID: previousTrackID,
                previousPositionMS: previousPositionMS,
                force: true
            )
            operations.status = PlaybackStatusText.afterTrackChange(
                isPlaying: snapshot.isPlaying,
                title: snapshot.currentTrack?.title
            )
        } catch {
            report(error)
        }
    }

    public func previousTrack() async {
        do {
            let previousTrackID = playback.nowPlaying?.id
            let previousPositionMS = playback.elapsedMS
            if playback.isPlaying {
                try playbackSystemIntegration?.prepareForPlayback()
            }
            let snapshot = try await invoke { try $0.previous() }
            apply(snapshot: snapshot)
            publishPlaybackPositionDiscontinuity(
                previousTrackID: previousTrackID,
                previousPositionMS: previousPositionMS,
                force: true
            )
            operations.status = PlaybackStatusText.afterTrackChange(
                isPlaying: snapshot.isPlaying,
                title: snapshot.currentTrack?.title
            )
        } catch {
            report(error)
        }
    }

    public func seek(toProgress progress: Double) async {
        guard let durationMS = playback.nowPlaying?.durationMS, durationMS > 0 else {
            return
        }
        let targetMS = Int(Double(durationMS) * min(max(progress, 0), 1))
        await seek(toMilliseconds: targetMS)
    }

    public func seek(toMilliseconds targetMS: Int) async {
        guard let durationMS = playback.nowPlaying?.durationMS, durationMS > 0 else {
            return
        }
        do {
            let previousTrackID = playback.nowPlaying?.id
            let previousPositionMS = playback.elapsedMS
            let clampedMS = min(max(targetMS, 0), durationMS)
            let snapshot = try await invoke { try $0.seek(positionMS: clampedMS) }
            apply(snapshot: snapshot)
            publishPlaybackPositionDiscontinuity(
                previousTrackID: previousTrackID,
                previousPositionMS: previousPositionMS,
                force: true
            )
        } catch {
            report(error)
        }
    }

    public func handleAudioInterruptionBegan() async {
        do {
            let snapshot = try await invoke { try $0.audioInterruptionBegan() }
            apply(snapshot: snapshot)
            if snapshot.currentTrack != nil {
                operations.status = "Playback interrupted"
            }
        } catch {
            report(error)
        }
    }

    public func handleAudioInterruptionEnded(systemShouldResume: Bool) async {
        var allowResume = PlaybackInterruptionPolicy.shouldPrepareForResume(
            systemShouldResume: systemShouldResume,
            resumeWasScheduled: resumeAfterAudioInterruption
        )
        if allowResume {
            do {
                try playbackSystemIntegration?.prepareForPlayback()
            } catch {
                allowResume = false
                report(error)
            }
        }

        do {
            let shouldResume = allowResume
            let snapshot = try await invoke {
                try $0.audioInterruptionEnded(systemShouldResume: shouldResume)
            }
            apply(snapshot: snapshot)
            operations.status = snapshot.isPlaying ? "Playback resumed" : "Playback paused"
        } catch {
            report(error)
        }
    }

    public func handleAudioOutputDisconnected() async {
        do {
            let snapshot = try await invoke { try $0.audioOutputDisconnected() }
            apply(snapshot: snapshot)
            if snapshot.currentTrack != nil {
                operations.status = "Paused because the audio output disconnected"
            }
        } catch {
            report(error)
        }
    }

    public func toggleShuffle() async {
        do {
            let enabled = !playback.isShuffleEnabled
            let snapshot = try await invoke { try $0.setShuffle(enabled: enabled) }
            apply(snapshot: snapshot)
            operations.status = snapshot.shuffleEnabled ? "Shuffle on" : "Shuffle off"
        } catch {
            report(error)
        }
    }

    public func cycleRepeatMode() async {
        await setRepeatMode(playback.repeatMode.next)
    }

    public func setRepeatMode(_ mode: PlaybackRepeatMode) async {
        do {
            let snapshot = try await invoke { try $0.setRepeatMode(mode) }
            apply(snapshot: snapshot)
            operations.status = snapshot.repeatMode.label
        } catch {
            report(error)
        }
    }
}
