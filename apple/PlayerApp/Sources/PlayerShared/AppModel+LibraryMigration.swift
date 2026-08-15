import Foundation

@MainActor
extension AppModel {
    public func exportLibrary(to packageURL: URL) async -> LibraryPackageSummary? {
        guard canStartLibraryMigration() else {
            return nil
        }
        var exportedSummary: LibraryPackageSummary?
        await runBusy("Exporting library") { [self] in
            let summary = try await invoke { try $0.exportLibrary(to: packageURL) }
            exportedSummary = summary
            operations.status = "Library exported"
            playback.detail = libraryPackageSummary(summary, location: packageURL)
        }
        return exportedSummary
    }

    public func importLibrary(from packageURL: URL) async {
        guard canStartLibraryMigration() else {
            return
        }
        await runBusy("Backing up current library") { [self] in
            let snapshot = try await invoke { try $0.pause() }
            apply(snapshot: snapshot)
            playbackSystemIntegration?.playbackDidStop()

            operations.status = "Preparing \(packageURL.lastPathComponent)"
            let localPackageURL = try await localLibraryPackageForImport(packageURL)
            let removesLocalPackage = localPackageURL != packageURL
            defer {
                if removesLocalPackage {
                    try? FileManager.default.removeItem(at: localPackageURL)
                }
            }

            let (backupURL, backupSummary) = try await backupCurrentLibrary()
            operations.lastLibraryBackupURL = backupURL
            operations.status = "Replacing current library"
            try await invoke { try $0.zeroOutLibrary() }

            operations.status = "Importing \(packageURL.lastPathComponent)"
            let imported = try await invoke { try $0.importLibrary(from: localPackageURL) }
            resetLibraryPresentation()
            await reloadActiveScope(quiet: true)
            await refreshPlaylists()
            await refreshPlaybackState()
            operations.status = "Library imported"
            playback.detail = "Imported \(imported.tracks) tracks. Backup: \(backupURL.path) (\(backupSummary.tracks) tracks)"
        }
    }

    public func zeroOutLibrary() async {
        guard canStartLibraryMigration() else {
            return
        }
        await runBusy("Clearing current library") { [self] in
            operations.status = "Clearing current library"
            try await invoke { try $0.zeroOutLibrary() }
            resetLibraryPresentation()
            resetPlaybackAfterLibraryReset()
            await reloadActiveScope(quiet: true)
            await refreshPlaylists()
            operations.status = "Library cleared"
            playback.detail = "Database and managed music files deleted"
        }
    }

    internal func canStartLibraryMigration() -> Bool {
        if operations.isLibraryWorking {
            operations.status = "Wait for the current library task to finish"
            return false
        }
        if operations.isAnalyzing {
            operations.status = "Stop loudness analysis before migrating the library"
            return false
        }
        return true
    }

    internal func backupCurrentLibrary() async throws -> (URL, LibraryPackageSummary) {
        let backupURL = try nextLibraryBackupURL()
        let summary = try await invoke { try $0.exportLibrary(to: backupURL) }
        return (backupURL, summary)
    }

    internal func localLibraryPackageForImport(_ packageURL: URL) async throws -> URL {
        #if os(iOS)
        return try await Task.detached(priority: .userInitiated) {
            let accessGranted = packageURL.startAccessingSecurityScopedResource()
            defer {
                if accessGranted {
                    packageURL.stopAccessingSecurityScopedResource()
                }
            }

            let stagingRoot = FileManager.default.temporaryDirectory
                .appendingPathComponent("SilentLibraryImports", isDirectory: true)
            try FileManager.default.createDirectory(
                at: stagingRoot,
                withIntermediateDirectories: true
            )
            let stagedPackage = stagingRoot
                .appendingPathComponent(
                    "\(UUID().uuidString).silentlibrary",
                    isDirectory: true
                )
            do {
                try FileManager.default.copyItem(at: packageURL, to: stagedPackage)
                return stagedPackage
            } catch {
                try? FileManager.default.removeItem(at: stagedPackage)
                throw error
            }
        }.value
        #else
        return packageURL
        #endif
    }

    internal func nextLibraryBackupURL() throws -> URL {
        let client = try requireClient()
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        let timestamp = formatter.string(from: Date())
        let suffix = UUID().uuidString.prefix(8)
        return client.dbURL
            .deletingLastPathComponent()
            .appendingPathComponent("Backups", isDirectory: true)
            .appendingPathComponent(
                "Silent-Library-\(timestamp)-\(suffix).silentlibrary",
                isDirectory: true
            )
    }

    internal func resetLibraryPresentation() {
        library.scope = .library
        playlists.sortMode = .defaultOrder
        library.query = ""
        library.selectedTrack = nil
        playback.nowPlaying = nil
        loadedTracks = []
        libraryPresentationCache = nil
        isPresentingCompleteLibrary = false
        library.tracks = []
        playlists.items = []
        playlists.recentItems = []
        playback.queue = []
        clearDetails()
    }

    internal func resetPlaybackAfterLibraryReset() {
        playback.isPlaying = false
        playback.isAudioInterrupted = false
        resumeAfterAudioInterruption = false
        playback.elapsedMS = 0
        playback.error = ""
        playback.queueCount = 0
        playback.queuePosition = nil
        playback.queue = []
        playback.playbackMode = .sequential
        playback.repeatMode = .off
        playback.isShuffleEnabled = false
        stopPlaybackTimer()
        playbackSystemIntegration?.playbackDidStop()
    }

    internal func libraryPackageSummary(
        _ summary: LibraryPackageSummary,
        location: URL
    ) -> String {
        "\(summary.tracks) tracks, \(summary.playlists) playlists, \(summary.audioFiles) audio files, \(summary.sidecarFiles) sidecars: \(location.path)"
    }
}
