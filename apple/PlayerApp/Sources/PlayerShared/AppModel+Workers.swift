import Foundation

@MainActor
extension AppModel {
    internal func refreshQueue() async {
        do {
            let queue = try await invoke { try $0.queueSnapshot() }
            playback.queue = queue.tracks
            playback.queueCount = queue.tracks.count
            playback.queuePosition = queue.currentIndex
            playback.playbackMode = queue.playbackMode
            playback.repeatMode = queue.repeatMode
            playback.isShuffleEnabled = queue.shuffleEnabled
        } catch {
            playback.error = error.localizedDescription
        }
    }

    internal func pollPlayback() async {
        guard !isPolling else {
            return
        }
        isPolling = true
        defer { isPolling = false }

        do {
            let previousTrackID = playback.nowPlaying?.id
            let previousPositionMS = playback.elapsedMS
            let previousPlaybackMode = playback.playbackMode
            let snapshot = try await invoke(priority: .utility) { try $0.poll() }
            apply(snapshot: snapshot)
            if previousTrackID != snapshot.currentTrack?.id
                || previousPlaybackMode != snapshot.playbackMode {
                await refreshQueue()
            }
            publishPlaybackPositionDiscontinuity(
                previousTrackID: previousTrackID,
                previousPositionMS: previousPositionMS
            )
        } catch {
            report(error)
        }
    }

    #if os(macOS)
    internal func handleLibraryWorker(_ event: LibraryWorkerEvent) {
        switch event {
        case .started(let operation, let total):
            operations.libraryProgress = total == 0 ? 1 : 0
            operations.libraryStatus = total == 0 ? "\(operation.rawValue.capitalized) found no tracks" : "\(operation.rawValue.capitalized) 0 / \(total)"
            operations.status = operations.libraryStatus
        case .trackFinished(
            let operation, let index, let total, let title, let imported, let copied,
            let duplicatesSkipped, let artworkCached, let metadataWarnings, let failures
        ):
            if total > 0 {
                operations.libraryProgress = Double(index) / Double(total)
                operations.libraryStatus = "\(operation.rawValue.capitalized) \(index) / \(total): \(title)"
            }
            if operation == .import {
                playback.detail = "Imported \(imported), copied \(copied), duplicates \(duplicatesSkipped), artwork \(artworkCached), warnings \(metadataWarnings), failures \(failures)"
            } else {
                playback.detail = "Audit failures \(failures)"
            }
        case .trackSkipped(let operation, let index, let total, let title, let reason, let duplicatesSkipped, let failures):
            if total > 0 {
                operations.libraryProgress = Double(index) / Double(total)
                operations.libraryStatus = "\(operation.rawValue.capitalized) \(index) / \(total): \(title)"
            }
            playback.detail = "Skipped: \(reason); duplicates \(duplicatesSkipped), failures \(failures)"
        case .trackFailed(_, let index, let total, _, let error):
            playback.error = error
            if total > 0 {
                operations.libraryProgress = Double(index) / Double(total)
            }
        case .mergeFinished(let duplicateGroups, let tracksMerged, let failures):
            playback.detail = "Audit groups \(duplicateGroups), merged \(tracksMerged), failures \(failures)"
        case .importFinished(let imported, let copied, let duplicatesSkipped, let artworkCached, let metadataWarnings, let failures):
            operations.isLibraryWorking = false
            operations.isBusy = false
            operations.libraryProgress = 1
            operations.libraryStatus = "Import finished"
            operations.status = "Import finished"
            playback.detail = "Imported \(imported), copied \(copied), duplicates \(duplicatesSkipped), artwork \(artworkCached), warnings \(metadataWarnings), failures \(failures)"
            Task {
                await reloadActiveScope(quiet: true)
                await refreshPlaylists()
            }
        case .auditFinished(let tracksScanned, let hashesUpdated, let duplicateGroups, let tracksMerged, let failures):
            operations.isLibraryWorking = false
            operations.isBusy = false
            operations.libraryProgress = 1
            operations.libraryStatus = "Audit finished"
            operations.status = "Audit finished"
            playback.detail = "Audit scanned \(tracksScanned), hashes \(hashesUpdated), groups \(duplicateGroups), merged \(tracksMerged), failures \(failures)"
            Task {
                await reloadActiveScope(quiet: true)
                await refreshPlaylists()
            }
        case .fatal(let error), .stderr(let error), .protocolError(let error):
            playback.error = error
            operations.status = "Library worker error"
        }
    }

    internal func handleLibraryWorkerExit(_ exitCode: Int32) async {
        libraryWorker = nil
        if exitCode == 0 {
            operations.isLibraryWorking = false
            operations.isBusy = false
            if operations.libraryProgress != 1 {
                operations.libraryProgress = 1
                operations.libraryStatus = "Library task finished"
                operations.status = "Library task finished"
                await reloadActiveScope(quiet: true)
                await refreshPlaylists()
            }
        } else if operations.isLibraryWorking {
            operations.isLibraryWorking = false
            operations.isBusy = false
            operations.libraryProgress = nil
            operations.status = "Library task stopped"
            playback.error = "Library worker exited with code \(exitCode)"
        }
    }
    #endif

    #if os(macOS)
    internal func handleAnalyzer(_ event: AnalyzerWorkerEvent) {
        switch event {
        case .started(let total):
            operations.analyzeProgress = total == 0 ? 1 : 0
            operations.analyzeStatus = total == 0 ? "No tracks need analysis" : "Analyzing 0 / \(total)"
            operations.status = total == 0 ? "Analysis finished" : "Analyzing in background"
        case .trackFinished(let index, let total, let title, let analyzed, let failed):
            if total > 0 {
                operations.analyzeProgress = Double(index) / Double(total)
                operations.analyzeStatus = "Analyzed \(index) / \(total): \(title)"
                playback.detail = "Loudness cache updated: \(analyzed) ok, \(failed) failed"
            }
        case .trackFailed(let index, let total, let title, let error):
            if total > 0 {
                operations.analyzeProgress = Double(index) / Double(total)
                operations.analyzeStatus = "Analyze failed \(index) / \(total): \(title)"
            }
            playback.error = error
        case .albumFinished(let albumsAnalyzed, let tracksUpdated):
            playback.detail = "Album loudness: \(albumsAnalyzed) albums, \(tracksUpdated) tracks"
        case .finished(let analyzed, let failed, let albumsAnalyzed):
            operations.isAnalyzing = false
            operations.analyzeProgress = 1
            operations.analyzeStatus = "Analysis finished: \(analyzed) ok, \(failed) failed"
            operations.status = "Analysis finished"
            playback.detail = "Tracks \(analyzed), albums \(albumsAnalyzed), failures \(failed)"
            Task {
                await reloadActiveScope(quiet: true)
            }
        case .fatal(let error), .stderr(let error), .protocolError(let error):
            playback.error = error
            operations.status = "Analyzer error"
        }
    }

    internal func handleAnalyzerExit(_ exitCode: Int32) async {
        analyzerWorker = nil
        if exitCode == 0 {
            operations.isAnalyzing = false
            if operations.analyzeProgress != 1 {
                operations.analyzeProgress = 1
                operations.analyzeStatus = "Analysis finished"
                operations.status = "Analysis finished"
                await reloadActiveScope(quiet: true)
            }
        } else if operations.isAnalyzing {
            operations.isAnalyzing = false
            operations.analyzeProgress = nil
            operations.status = "Analyzer stopped"
            playback.error = "Analyzer exited with code \(exitCode)"
        }
    }
    #endif
}
