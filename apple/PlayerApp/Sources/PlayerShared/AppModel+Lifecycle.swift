import Foundation

@MainActor
extension AppModel {
    public func shutdownForQuit() {
        stopPlaybackTimer()
        #if os(macOS)
        analyzerWorker?.stop()
        analyzerWorker = nil
        libraryWorker?.stop()
        libraryWorker = nil
        #endif
        detailsTask?.cancel()
        detailsTask = nil
        playbackSystemIntegration?.shutdown()
        playbackSystemIntegration = nil

        if let client {
            _ = try? client.pause()
        }
    }

    public func installPlaybackSystemIntegration(_ integration: any PlaybackSystemIntegration) {
        playbackSystemIntegration?.shutdown()
        playbackSystemIntegration = integration
        integration.start()
    }

    public func refreshPlaylists() async {
        do {
            playlists.items = try await invoke { try $0.playlists() }
            playlists.recentItems = try await invoke { try $0.recentPlaylists(limit: 6) }
        } catch {
            playback.error = error.localizedDescription
        }
    }

    public func refreshPlaybackState() async {
        do {
            let snapshot = try await invoke { try $0.poll() }
            apply(snapshot: snapshot)
            await refreshQueue()
        } catch {
            playback.error = error.localizedDescription
        }
    }

    public func applicationDidEnterBackground() {
        guard playback.isPlaying else {
            return
        }
        do {
            try playbackSystemIntegration?.applicationDidEnterBackground()
        } catch {
            report(error)
        }
    }
}
