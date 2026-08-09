import Foundation

@MainActor
extension AppModel {
    public func analyzeLibrary() async {
        #if os(macOS)
        if operations.isAnalyzing {
            stopAnalyze()
            return
        }

        guard let client else {
            report(serviceUnavailableError)
            return
        }
        let worker = AnalyzerWorker(
            dbURL: client.dbURL,
            repoRoot: client.repoRoot,
            onEvent: { [weak self] event in
                Task { @MainActor in
                    self?.handleAnalyzer(event)
                }
            },
            onExit: { [weak self] exitCode in
                Task { @MainActor in
                    await self?.handleAnalyzerExit(exitCode)
                }
            }
        )

        do {
            analyzerWorker = worker
            operations.isAnalyzing = true
            operations.analyzeProgress = nil
            operations.analyzeStatus = "Starting loudness analyzer"
            playback.error = ""
            operations.status = "Analyzing in background"
            try worker.start()
            invalidateLibraryPresentationCache()
        } catch {
            analyzerWorker = nil
            operations.isAnalyzing = false
            operations.analyzeProgress = nil
            report(error)
        }
        #else
        if operations.isAnalyzing {
            operations.status = "Analysis is already running"
            return
        }
        operations.isAnalyzing = true
        operations.analyzeProgress = nil
        operations.analyzeStatus = "Analyzing loudness"
        await runBusy("Analyzing loudness") { [self] in
            let summary = try await invoke { try $0.analyze() }
            invalidateLibraryPresentationCache()
            operations.analyzeStatus = "Analyzed \(summary.tracksAnalyzed), failed \(summary.trackFailures)"
            operations.status = "Analysis finished"
            playback.detail = "Albums \(summary.albumsAnalyzed), album tracks \(summary.albumTracksUpdated), skipped \(summary.albumSkipped)"
            await reloadActiveScope(quiet: true)
        }
        operations.isAnalyzing = false
        operations.analyzeProgress = nil
        #endif
    }

    public func stopAnalyze() {
        #if os(macOS)
        guard let analyzerWorker else {
            return
        }

        analyzerWorker.stop()
        self.analyzerWorker = nil
        operations.isAnalyzing = false
        operations.analyzeProgress = nil
        operations.analyzeStatus = "Analysis stopped"
        operations.status = "Analysis stopped"
        Task {
            await reloadActiveScope(quiet: true)
        }
        #else
        operations.status = "Analysis cannot be interrupted on iPhone yet"
        #endif
    }

    #if os(macOS)
    internal func startLibraryWorker(_ operation: LibraryWorkerOperation, status startStatus: String) {
        if operations.isLibraryWorking {
            stopLibraryWork()
            return
        }

        guard let client else {
            report(serviceUnavailableError)
            return
        }
        let worker = LibraryWorker(
            operation: operation,
            dbURL: client.dbURL,
            mediaRootURL: client.mediaRootURL,
            repoRoot: client.repoRoot,
            onEvent: { [weak self] event in
                Task { @MainActor in
                    self?.handleLibraryWorker(event)
                }
            },
            onExit: { [weak self] exitCode in
                Task { @MainActor in
                    await self?.handleLibraryWorkerExit(exitCode)
                }
            }
        )

        do {
            libraryWorker = worker
            operations.isLibraryWorking = true
            operations.isBusy = true
            operations.libraryProgress = nil
            operations.libraryStatus = startStatus
            playback.error = ""
            operations.status = startStatus
            try worker.start()
            invalidateLibraryPresentationCache()
        } catch {
            libraryWorker = nil
            operations.isLibraryWorking = false
            operations.isBusy = false
            operations.libraryProgress = nil
            report(error)
        }
    }
    #endif
}
