import Combine
import Foundation

public enum LibraryScope: Hashable, Sendable {
    case library
    case favorites
    case history
    case playlist(String)

    public var title: String {
        switch self {
        case .library:
            return "Library"
        case .favorites:
            return "Favorites"
        case .history:
            return "History"
        case .playlist(let name):
            return name
        }
    }
}

public enum PlaylistSortMode: CaseIterable, Identifiable, Sendable {
    case defaultOrder
    case title
    case artist
    case album
    case rating

    public var id: String {
        apiValue
    }

    public var apiValue: String {
        switch self {
        case .defaultOrder:
            return "manual"
        case .title:
            return "title"
        case .artist:
            return "artist"
        case .album:
            return "album"
        case .rating:
            return "rating"
        }
    }

    public var label: String {
        switch self {
        case .defaultOrder:
            return "Default"
        case .title:
            return "Name"
        case .artist:
            return "Artist"
        case .album:
            return "Album"
        case .rating:
            return "Rating"
        }
    }

    public var systemImage: String {
        switch self {
        case .defaultOrder:
            return "line.3.horizontal"
        case .title:
            return "textformat"
        case .artist:
            return "person"
        case .album:
            return "opticaldisc"
        case .rating:
            return "star"
        }
    }
}

public enum PlaybackRepeatMode: String, CaseIterable, Codable, Identifiable, Sendable {
    case off
    case all
    case one

    public var id: String {
        rawValue
    }

    public var apiValue: String {
        rawValue
    }

    public var label: String {
        switch self {
        case .off:
            return "Order"
        case .all:
            return "Repeat All"
        case .one:
            return "Repeat One"
        }
    }

    public var systemImage: String {
        switch self {
        case .off, .all:
            return "repeat"
        case .one:
            return "repeat.1"
        }
    }

    public var next: PlaybackRepeatMode {
        switch self {
        case .off:
            return .all
        case .all:
            return .one
        case .one:
            return .off
        }
    }
}

enum PlaybackStatusText {
    static func afterTrackChange(isPlaying: Bool, title: String?) -> String {
        let trimmedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines)
        let trackName = trimmedTitle.flatMap { $0.isEmpty ? nil : $0 } ?? "track"
        return isPlaying ? "Playing \(trackName)" : "Paused at \(trackName)"
    }
}

public struct TrackViewChoice: Identifiable, Hashable, Sendable {
    public let id: String
    public let track: TrackItem
    public let index: Int
    public let total: Int
}

@MainActor
public final class AppModel: ObservableObject {
    @Published public var tracks: [TrackItem] = []
    @Published public var playlists: [PlaylistItem] = []
    @Published public var selectedTrack: TrackItem?
    @Published public var query: String = ""
    @Published public var status: String = "Ready"
    @Published public var isBusy: Bool = false
    @Published public var isPlaying: Bool = false
    @Published public var isAudioInterrupted: Bool = false
    @Published public var nowPlaying: TrackItem?
    @Published public var nowPlayingDetails: TrackDetails?
    @Published public var isLoadingDetails: Bool = false
    @Published public var isViewEditPresented: Bool = false
    @Published public var isViewSaving: Bool = false
    @Published public var viewEditNameDraft: String = ""
    @Published public var viewEditTitleDraft: String = ""
    @Published public var viewEditArtistDraft: String = ""
    @Published public var viewEditAlbumDraft: String = ""
    @Published public var viewEditNotesDraft: String = ""
    @Published public var viewEditArtworkURL: URL?
    @Published public var viewEditLyricsURL: URL?
    @Published public var playbackElapsedMS: Int = 0
    @Published public var playbackError: String = ""
    @Published public var playbackDetail: String = ""
    @Published public var repeatMode: PlaybackRepeatMode = .off
    @Published public var isShuffleEnabled: Bool = false
    @Published public var queueCount: Int = 0
    @Published public var queuePosition: Int?
    @Published public var libraryScope: LibraryScope = .library
    @Published public var isPlaylistCreatePresented: Bool = false
    @Published public var newPlaylistNameDraft: String = "New Playlist"
    @Published public var isPlaylistSettingsPresented: Bool = false
    @Published public var playlistSettingsOriginalName: String?
    @Published public var playlistSettingsNameDraft: String = ""
    @Published public var playlistSettingsArtworkURL: URL?
    @Published public var playlistSettingsCurrentArtworkURL: URL?
    @Published public var playlistSortMode: PlaylistSortMode = .defaultOrder
    @Published public var isPlaylistPickerPresented: Bool = false
    @Published public var playlistPickerTrack: TrackItem?
    @Published public var isAnalyzing: Bool = false
    @Published public var analyzeProgress: Double?
    @Published public var analyzeStatus: String = ""
    @Published public var isLibraryWorking: Bool = false
    @Published public var libraryProgress: Double?
    @Published public var libraryStatus: String = ""
    @Published public var lastLibraryBackupURL: URL?

    private let client: RustPlayerClient
    private var playbackSystemIntegration: (any PlaybackSystemIntegration)?
    private var resumeAfterAudioInterruption = false
    nonisolated(unsafe) private var playbackTimer: Timer?
    private var isPolling = false
    #if os(macOS)
    private var analyzerWorker: AnalyzerWorker?
    private var libraryWorker: LibraryWorker?
    #endif
    private var detailsTask: Task<Void, Never>?
    private var detailsTrackID: String?
    private var loadingDetailsTrackID: String?
    private var allTrackViews: [TrackItem] = []
    private var activeViewIDByPrimaryID: [String: String] = [:]

    public init(client: RustPlayerClient? = nil) {
        do {
            self.client = try client ?? RustPlayerClient.discover()
        } catch {
            fatalError(error.localizedDescription)
        }
    }

    deinit {
        playbackTimer?.invalidate()
        #if os(macOS)
        analyzerWorker?.stop()
        libraryWorker?.stop()
        #endif
        detailsTask?.cancel()
    }

    public var dbPath: String {
        client.dbURL.path
    }

    public var mediaRootPath: String {
        client.mediaRootURL.path
    }

    public var repoPath: String {
        client.repoRoot.path
    }

    public var isPaused: Bool {
        nowPlaying != nil && !isPlaying
    }

    public var playbackProgress: Double? {
        guard let durationMS = nowPlaying?.durationMS, durationMS > 0 else {
            return nil
        }
        return min(max(Double(playbackElapsedMS) / Double(durationMS), 0), 1)
    }

    public var playbackTimeText: String {
        "\(formatTime(playbackElapsedMS)) / \(nowPlaying?.durationText ?? "--:--")"
    }

    public var normalizeText: String {
        if let gainDB = nowPlaying?.gainDB {
            return String(format: "Normalize %@ dB", String(format: "%+.1f", gainDB))
        }
        return nowPlaying?.loudnessStatus ?? "Normalize pending"
    }

    public var queueStatusText: String {
        guard queueCount > 0 else {
            return "Queue empty"
        }
        if let queuePosition {
            return "Queue \(queuePosition + 1) / \(queueCount)"
        }
        return "Queue \(queueCount)"
    }

    public var activePlaylistName: String? {
        if case .playlist(let name) = libraryScope {
            return name
        }
        return nil
    }

    public var detailTrack: TrackItem? {
        selectedTrack ?? nowPlaying
    }

    public var detailDetails: TrackDetails? {
        guard let track = detailTrack else {
            return nil
        }
        return matchingDetails(for: track)
    }

    public var detailViewChoices: [TrackViewChoice] {
        guard let track = detailTrack else {
            return []
        }
        return viewChoices(for: track)
    }

    public var viewEditChanged: Bool {
        guard let track = detailTrack else {
            return false
        }
        let details = matchingDetails(for: track)
        return normalizedDraft(viewEditNameDraft) != normalizedDraft(details?.viewName ?? track.viewName)
            || viewEditTitleDraft != (details?.displayTitle ?? track.title)
            || viewEditArtistDraft != (details?.displayArtist ?? track.artist)
            || viewEditAlbumDraft != (details?.displayAlbum ?? track.album)
            || viewEditNotesDraft != (details?.notes ?? "")
            || viewEditArtworkURL != nil
            || viewEditLyricsURL != nil
    }

    public var playlistSettingsChanged: Bool {
        guard let originalName = playlistSettingsOriginalName else {
            return false
        }
        return normalizedDraft(playlistSettingsNameDraft) != originalName
            || playlistSettingsArtworkURL != nil
    }

    public var isShowingInitialDetailsLoad: Bool {
        isLoadingDetails && nowPlayingDetails == nil
    }

    public func bootstrap() async {
        await reloadActiveScope()
        await refreshPlaylists()
    }

    public func exportLibrary(to packageURL: URL) async -> LibraryPackageSummary? {
        guard canStartLibraryMigration() else {
            return nil
        }
        var exportedSummary: LibraryPackageSummary?
        await runBusy("Exporting library") { [self] in
            let summary = try await invoke { try $0.exportLibrary(to: packageURL) }
            exportedSummary = summary
            status = "Library exported"
            playbackDetail = libraryPackageSummary(summary, location: packageURL)
        }
        return exportedSummary
    }

    public func importLibrary(from packageURL: URL) async {
        guard canStartLibraryMigration() else {
            return
        }
        await runBusy("Backing up current library") { [self] in
            let snapshot = try await invoke { try $0.stop() }
            apply(snapshot: snapshot)
            playbackSystemIntegration?.playbackDidStop()

            status = "Preparing \(packageURL.lastPathComponent)"
            let localPackageURL = try await localLibraryPackageForImport(packageURL)
            let removesLocalPackage = localPackageURL != packageURL
            defer {
                if removesLocalPackage {
                    try? FileManager.default.removeItem(at: localPackageURL)
                }
            }

            let (backupURL, backupSummary) = try await backupCurrentLibrary()
            lastLibraryBackupURL = backupURL
            status = "Replacing current library"
            try await invoke { try $0.zeroOutLibrary() }

            status = "Importing \(packageURL.lastPathComponent)"
            let imported = try await invoke { try $0.importLibrary(from: localPackageURL) }
            resetLibraryPresentation()
            await reloadActiveScope(quiet: true)
            await refreshPlaylists()
            status = "Library imported"
            playbackDetail = "Imported \(imported.tracks) tracks. Backup: \(backupURL.path) (\(backupSummary.tracks) tracks)"
        }
    }

    public func zeroOutLibrary() async {
        guard canStartLibraryMigration() else {
            return
        }
        await runBusy("Backing up current library") { [self] in
            let snapshot = try await invoke { try $0.stop() }
            apply(snapshot: snapshot)
            playbackSystemIntegration?.playbackDidStop()

            let (backupURL, backupSummary) = try await backupCurrentLibrary()
            lastLibraryBackupURL = backupURL
            status = "Clearing current library"
            try await invoke { try $0.zeroOutLibrary() }
            resetLibraryPresentation()
            await reloadActiveScope(quiet: true)
            await refreshPlaylists()
            status = "Library cleared"
            playbackDetail = "Backup: \(backupURL.path) (\(backupSummary.tracks) tracks)"
        }
    }

    private func canStartLibraryMigration() -> Bool {
        if isLibraryWorking {
            status = "Wait for the current library task to finish"
            return false
        }
        if isAnalyzing {
            status = "Stop loudness analysis before migrating the library"
            return false
        }
        return true
    }

    private func backupCurrentLibrary() async throws -> (URL, LibraryPackageSummary) {
        let backupURL = nextLibraryBackupURL()
        let summary = try await invoke { try $0.exportLibrary(to: backupURL) }
        return (backupURL, summary)
    }

    private func localLibraryPackageForImport(_ packageURL: URL) async throws -> URL {
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

    private func nextLibraryBackupURL() -> URL {
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

    private func resetLibraryPresentation() {
        libraryScope = .library
        playlistSortMode = .defaultOrder
        query = ""
        selectedTrack = nil
        nowPlaying = nil
        allTrackViews = []
        activeViewIDByPrimaryID = [:]
        tracks = []
        playlists = []
        clearDetails()
    }

    private func libraryPackageSummary(
        _ summary: LibraryPackageSummary,
        location: URL
    ) -> String {
        "\(summary.tracks) tracks, \(summary.audioFiles) audio files, \(summary.sidecarFiles) sidecars: \(location.path)"
    }

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
            status = "Imported \(summary.imported), duplicates \(summary.duplicatesSkipped)"
            playbackDetail = "Copied \(summary.copied), artwork \(summary.artworkCached), warnings \(summary.metadataWarnings)"
            await reloadActiveScope(quiet: true)
            await refreshPlaylists()
        }
        #endif
    }

    public func importFiles(_ files: [URL]) async {
        let files = files.filter { !$0.path.isEmpty }
        guard !files.isEmpty else {
            status = "No files selected"
            playbackDetail = ""
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
            writeImportDebugLog(
                "importFiles summary imported=\(summary.imported) copied=\(summary.copied) duplicates=\(summary.duplicatesSkipped) warnings=\(summary.metadataWarnings)"
            )
            status = "Imported \(summary.imported), duplicates \(summary.duplicatesSkipped)"
            playbackDetail = "Copied \(summary.copied), artwork \(summary.artworkCached), warnings \(summary.metadataWarnings)"
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
        isLibraryWorking = false
        isBusy = false
        libraryProgress = nil
        libraryStatus = "Library task stopped"
        status = "Library task stopped"
        Task {
            await reloadActiveScope(quiet: true)
            await refreshPlaylists()
        }
        #else
        status = "Current library task cannot be interrupted"
        #endif
    }

    public func auditDatabase() async {
        #if os(macOS)
        startLibraryWorker(.audit, status: "Auditing database")
        #else
        await runBusy("Auditing database") { [self] in
            let summary = try await invoke { try $0.auditDatabase() }
            status = "Audit finished"
            playbackDetail = "Scanned \(summary.tracksScanned), hashes \(summary.hashesUpdated), groups \(summary.duplicateGroups), merged \(summary.tracksMerged), failures \(summary.failures)"
            await reloadActiveScope(quiet: true)
            await refreshPlaylists()
        }
        #endif
    }

    public func refreshLibrary(quiet: Bool = false) async {
        libraryScope = .library
        playlistSortMode = .defaultOrder
        await reloadActiveScope(quiet: quiet)
    }

    public func showFavorites() async {
        libraryScope = .favorites
        playlistSortMode = .defaultOrder
        query = ""
        await reloadActiveScope()
    }

    public func showHistory() async {
        libraryScope = .history
        playlistSortMode = .defaultOrder
        query = ""
        await reloadActiveScope()
    }

    public func showPlaylist(_ playlist: PlaylistItem) async {
        libraryScope = .playlist(playlist.name)
        playlistSortMode = .defaultOrder
        query = ""
        await reloadActiveScope()
    }

    public func reloadActiveScope(quiet: Bool = false) async {
        await reloadActiveScope(quiet: quiet, preferredSelectedViewID: nil, forceDetails: false)
    }

    private func reloadActiveScope(
        quiet: Bool = false,
        preferredSelectedViewID: String?,
        forceDetails: Bool,
        preferredSelectedView: TrackItem? = nil
    ) async {
        await runBusy(quiet ? nil : "Loading \(libraryScope.title)") { [self] in
            var loaded: [TrackItem]
            switch libraryScope {
            case .library:
                loaded = try await invoke { try $0.library() }
            case .favorites:
                loaded = try await invoke { try $0.favorites() }
            case .history:
                loaded = try await invoke { try $0.history() }
            case .playlist(let name):
                loaded = try await invoke { try $0.playlistTracks(name: name) }
            }
            if let preferredSelectedView,
               !loaded.contains(where: { $0.id == preferredSelectedView.id }),
               loaded.contains(where: { $0.primaryViewID == preferredSelectedView.primaryViewID }) {
                loaded.append(preferredSelectedView)
            }
            applyLoadedViews(
                loaded,
                preferredSelectedViewID: preferredSelectedView?.id ?? preferredSelectedViewID
            )
            if let selectedTrack {
                loadDetails(for: selectedTrack, force: forceDetails)
            } else if let nowPlaying {
                loadDetails(for: nowPlaying, force: forceDetails)
            }
            status = loaded.isEmpty
                ? "\(libraryScope.title) is empty"
                : "\(libraryScope.title): \(tracks.count) tracks, \(loaded.count) views"
        }
    }

    public func selectTrack(id: String?) {
        let newSelection = id.flatMap { id in tracks.first(where: { $0.id == id }) }
        if let newSelection, selectedTrack?.id == newSelection.id {
            loadDetails(for: newSelection)
            return
        }

        selectedTrack = newSelection
        if let selectedTrack {
            setActiveView(selectedTrack)
            tracks = visibleTracks(from: allTrackViews)
        }
        if let selectedTrack {
            loadDetails(for: selectedTrack)
        } else if let nowPlaying {
            loadDetails(for: nowPlaying)
        } else {
            clearDetails()
        }
    }

    public func selectDetailView(id: String) {
        guard let view = allTrackViews.first(where: { $0.id == id })
            ?? detailViewChoices.first(where: { $0.id == id })?.track
        else {
            status = "Selected view is unavailable"
            return
        }
        setActiveView(view)
        selectedTrack = view
        tracks = visibleTracks(from: allTrackViews)
        loadDetails(for: view, force: true)
    }

    public func search() async {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            await reloadActiveScope()
            return
        }

        libraryScope = .library
        await runBusy("Searching") { [self] in
            let loaded = try await invoke { try $0.search(trimmed, limit: 200) }
            applyLoadedViews(loaded, preferredSelectedViewID: selectedTrack?.id)
            status = "Search returned \(tracks.count) tracks, \(loaded.count) views"
        }
    }

    public func analyzeLibrary() async {
        #if os(macOS)
        if isAnalyzing {
            stopAnalyze()
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
            isAnalyzing = true
            analyzeProgress = nil
            analyzeStatus = "Starting loudness analyzer"
            playbackError = ""
            status = "Analyzing in background"
            try worker.start()
        } catch {
            analyzerWorker = nil
            isAnalyzing = false
            analyzeProgress = nil
            report(error)
        }
        #else
        if isAnalyzing {
            status = "Analysis is already running"
            return
        }
        isAnalyzing = true
        analyzeProgress = nil
        analyzeStatus = "Analyzing loudness"
        await runBusy("Analyzing loudness") { [self] in
            let summary = try await invoke { try $0.analyze() }
            analyzeStatus = "Analyzed \(summary.tracksAnalyzed), failed \(summary.trackFailures)"
            status = "Analysis finished"
            playbackDetail = "Albums \(summary.albumsAnalyzed), album tracks \(summary.albumTracksUpdated), skipped \(summary.albumSkipped)"
            await reloadActiveScope(quiet: true)
        }
        isAnalyzing = false
        analyzeProgress = nil
        #endif
    }

    public func stopAnalyze() {
        #if os(macOS)
        guard let analyzerWorker else {
            return
        }

        analyzerWorker.stop()
        self.analyzerWorker = nil
        isAnalyzing = false
        analyzeProgress = nil
        analyzeStatus = "Analysis stopped"
        status = "Analysis stopped"
        Task {
            await reloadActiveScope(quiet: true)
        }
        #else
        status = "Analysis cannot be interrupted on iPhone yet"
        #endif
    }

    #if os(macOS)
    private func startLibraryWorker(_ operation: LibraryWorkerOperation, status startStatus: String) {
        if isLibraryWorking {
            stopLibraryWork()
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
            isLibraryWorking = true
            isBusy = true
            libraryProgress = nil
            libraryStatus = startStatus
            playbackError = ""
            status = startStatus
            try worker.start()
        } catch {
            libraryWorker = nil
            isLibraryWorking = false
            isBusy = false
            libraryProgress = nil
            report(error)
        }
    }
    #endif

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

        let client = self.client
        Task.detached(priority: .background) {
            _ = try? client.stop()
        }
    }

    public func installPlaybackSystemIntegration(_ integration: any PlaybackSystemIntegration) {
        playbackSystemIntegration?.shutdown()
        playbackSystemIntegration = integration
        integration.start()
    }

    public func refreshPlaylists() async {
        do {
            playlists = try await invoke { try $0.playlists() }
        } catch {
            playbackError = error.localizedDescription
        }
    }

    public func presentCreatePlaylist() {
        isPlaylistPickerPresented = false
        newPlaylistNameDraft = defaultNewPlaylistName()
        isPlaylistCreatePresented = true
    }

    public func cancelCreatePlaylist() {
        isPlaylistCreatePresented = false
    }

    public func createPlaylist() async {
        let name = newPlaylistNameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else {
            status = "Playlist name is empty"
            return
        }

        await runBusy("Creating \(name)") { [self] in
            try await invoke { try $0.createPlaylist(name: name) }
            libraryScope = .playlist(name)
            playlistSortMode = .defaultOrder
            status = "Created \(name)"
            isPlaylistCreatePresented = false
            await refreshPlaylists()
            await reloadActiveScope(quiet: true)
        }
    }

    public func presentPlaylistSettings(_ playlist: PlaylistItem) {
        playlistSettingsOriginalName = playlist.name
        playlistSettingsNameDraft = playlist.name
        playlistSettingsArtworkURL = nil
        playlistSettingsCurrentArtworkURL = playlist.artworkURL
        isPlaylistSettingsPresented = true
    }

    public func cancelPlaylistSettings() {
        isPlaylistSettingsPresented = false
        clearPlaylistSettingsDraft()
    }

    public func setPlaylistSettingsArtworkURL(_ imageURL: URL) {
        playlistSettingsArtworkURL = imageURL
    }

    public func savePlaylistSettings() async {
        guard let oldName = playlistSettingsOriginalName else {
            status = "Select a playlist first"
            return
        }
        let newName = playlistSettingsNameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !newName.isEmpty else {
            status = "Playlist name is empty"
            return
        }
        let artworkURL = playlistSettingsArtworkURL

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
                if libraryScope == .playlist(oldName) {
                    libraryScope = .playlist(newName)
                }
            }
            if let artworkURL {
                let artworkPlaylistName = currentName
                try await invoke { try $0.setPlaylistArtwork(name: artworkPlaylistName, imageURL: artworkURL) }
            }

            if didRename && artworkURL != nil {
                status = "Updated \(currentName)"
            } else if didRename {
                status = "Renamed playlist"
            } else if artworkURL != nil {
                status = "Updated \(currentName) artwork"
            } else {
                status = "Playlist unchanged"
            }
            isPlaylistSettingsPresented = false
            clearPlaylistSettingsDraft()
            await refreshPlaylists()
            if libraryScope == .playlist(currentName) {
                await reloadActiveScope(quiet: true)
            }
        }
    }

    public func setPlaylistArtwork(_ playlist: PlaylistItem, imageURL: URL) async {
        await setPlaylistArtwork(name: playlist.name, imageURL: imageURL)
    }

    private func setPlaylistArtwork(name: String, imageURL: URL) async {
        await runBusy("Setting playlist artwork") { [self] in
            let accessGranted = imageURL.startAccessingSecurityScopedResource()
            defer {
                if accessGranted {
                    imageURL.stopAccessingSecurityScopedResource()
                }
            }
            try await invoke { try $0.setPlaylistArtwork(name: name, imageURL: imageURL) }
            status = "Updated \(name) artwork"
            await refreshPlaylists()
        }
    }

    public func addSelectedToPlaylist() async {
        guard let track = selectedTrack ?? nowPlaying else {
            status = "Select a track first"
            return
        }
        guard let name = activePlaylistName else {
            status = "Open a playlist first"
            return
        }

        await add(track, toPlaylistNamed: name)
    }

    public func presentPlaylistPicker(for track: TrackItem? = nil) {
        guard let target = track ?? detailTrack else {
            status = "Select or play a track first"
            return
        }
        playlistPickerTrack = target
        isPlaylistPickerPresented = true
    }

    public func cancelPlaylistPicker() {
        isPlaylistPickerPresented = false
        playlistPickerTrack = nil
    }

    public func addPlaylistPickerTrack(to playlist: PlaylistItem) async {
        guard let track = playlistPickerTrack ?? detailTrack else {
            status = "Select or play a track first"
            return
        }

        await add(track, toPlaylistNamed: playlist.name)
        isPlaylistPickerPresented = false
        playlistPickerTrack = nil
    }

    private func add(_ track: TrackItem, toPlaylistNamed name: String) async {
        await runBusy("Adding to \(name)") { [self] in
            try await invoke { try $0.addToPlaylist(name: name, path: track.path) }
            status = "Added \(track.title) to \(name)"
            await refreshPlaylists()
            if libraryScope == .playlist(name) {
                await reloadActiveScope(quiet: true)
            }
        }
    }

    public func removeSelectedFromActivePlaylist() async {
        guard let name = activePlaylistName else {
            status = "Select a playlist first"
            return
        }
        guard let track = selectedTrack else {
            status = "Select a track first"
            return
        }
        await runBusy("Removing from playlist") { [self] in
            try await invoke { try $0.removeFromPlaylist(name: name, path: track.path) }
            status = "Removed \(track.title)"
            await refreshPlaylists()
            await reloadActiveScope(quiet: true)
        }
    }

    public func moveSelectedInActivePlaylist(delta: Int) async {
        guard let name = activePlaylistName else {
            status = "Select a playlist first"
            return
        }
        guard let track = selectedTrack else {
            status = "Select a track first"
            return
        }
        await runBusy(nil) { [self] in
            try await invoke { try $0.movePlaylistTrack(name: name, path: track.path, delta: delta) }
            playlistSortMode = .defaultOrder
            status = "Moved \(track.title)"
            await reloadActiveScope(quiet: true)
        }
    }

    public func sortVisibleTracks(_ sortMode: PlaylistSortMode) async {
        playlistSortMode = sortMode

        guard let name = activePlaylistName else {
            tracks = visibleTracks(from: allTrackViews)
            status = sortMode == .defaultOrder
                ? "\(libraryScope.title) default order"
                : "Sorted \(libraryScope.title) by \(sortMode.label)"
            return
        }

        await runBusy("Sorting \(name)") { [self] in
            try await invoke { try $0.sortPlaylist(name: name, sort: sortMode.apiValue) }
            status = "Sorted \(name) by \(sortMode.label)"
            await reloadActiveScope(quiet: true)
        }
    }

    public func clearActivePlaylist() async {
        guard let name = activePlaylistName else {
            status = "Select a playlist first"
            return
        }
        await runBusy("Clearing playlist") { [self] in
            try await invoke { try $0.clearPlaylist(name: name) }
            selectedTrack = nil
            clearDetails()
            status = "Cleared \(name)"
            await refreshPlaylists()
            await reloadActiveScope(quiet: true)
        }
    }

    public func deleteActivePlaylist() async {
        guard let name = activePlaylistName else {
            status = "Select a playlist first"
            return
        }
        await runBusy("Deleting playlist") { [self] in
            try await invoke { try $0.deletePlaylist(name: name) }
            libraryScope = .library
            selectedTrack = nil
            clearDetails()
            status = "Deleted \(name)"
            await refreshPlaylists()
            await reloadActiveScope(quiet: true)
        }
    }

    public func playSelected() async {
        guard let track = selectedTrack else {
            status = "Select a track first"
            return
        }
        await play(track)
    }

    public func play(_ track: TrackItem) async {
        guard !isAudioInterrupted else {
            status = "Wait for the audio interruption to end"
            return
        }
        await runBusy(nil) { [self] in
            try playbackSystemIntegration?.prepareForPlayback()
            let queuePaths = tracks.map(\.path)
            let paths = queuePaths.contains(track.path) ? queuePaths : [track.path]
            let snapshot = try await invoke { try $0.playQueue(paths: paths, startPath: track.path) }
            selectedTrack = track
            apply(snapshot: snapshot, fallbackTrack: track)
            status = "Playing \(track.title)"
        }
    }

    public func pauseOrResume() async {
        guard nowPlaying != nil else {
            await playSelected()
            return
        }
        guard isPlaying || !isAudioInterrupted else {
            status = "Wait for the audio interruption to end"
            return
        }

        do {
            let snapshot: PlaybackSnapshot
            if isPlaying {
                snapshot = try await invoke { try $0.pause() }
                status = "Paused"
            } else {
                try playbackSystemIntegration?.prepareForPlayback()
                snapshot = try await invoke { try $0.resume() }
                status = "Playing \(snapshot.currentTrack?.title ?? nowPlaying?.title ?? "")"
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
            playbackSystemIntegration?.playbackDidStop()
            status = "Stopped"
        } catch {
            report(error)
        }
    }

    public func nextTrack() async {
        do {
            if isPlaying {
                try playbackSystemIntegration?.prepareForPlayback()
            }
            let snapshot = try await invoke { try $0.next() }
            apply(snapshot: snapshot)
            status = PlaybackStatusText.afterTrackChange(
                isPlaying: snapshot.isPlaying,
                title: snapshot.currentTrack?.title
            )
        } catch {
            report(error)
        }
    }

    public func previousTrack() async {
        do {
            if isPlaying {
                try playbackSystemIntegration?.prepareForPlayback()
            }
            let snapshot = try await invoke { try $0.previous() }
            apply(snapshot: snapshot)
            status = PlaybackStatusText.afterTrackChange(
                isPlaying: snapshot.isPlaying,
                title: snapshot.currentTrack?.title
            )
        } catch {
            report(error)
        }
    }

    public func seek(toProgress progress: Double) async {
        guard let durationMS = nowPlaying?.durationMS, durationMS > 0 else {
            return
        }
        let targetMS = Int(Double(durationMS) * min(max(progress, 0), 1))
        await seek(toMilliseconds: targetMS)
    }

    public func seek(toMilliseconds targetMS: Int) async {
        guard let durationMS = nowPlaying?.durationMS, durationMS > 0 else {
            return
        }
        do {
            let clampedMS = min(max(targetMS, 0), durationMS)
            let snapshot = try await invoke { try $0.seek(positionMS: clampedMS) }
            apply(snapshot: snapshot)
        } catch {
            report(error)
        }
    }

    public func handleAudioInterruptionBegan() async {
        do {
            let snapshot = try await invoke { try $0.audioInterruptionBegan() }
            apply(snapshot: snapshot)
            if snapshot.currentTrack != nil {
                status = "Playback interrupted"
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
            status = snapshot.isPlaying ? "Playback resumed" : "Playback paused"
        } catch {
            report(error)
        }
    }

    public func handleAudioOutputDisconnected() async {
        do {
            let snapshot = try await invoke { try $0.audioOutputDisconnected() }
            apply(snapshot: snapshot)
            if snapshot.currentTrack != nil {
                status = "Paused because the audio output disconnected"
            }
        } catch {
            report(error)
        }
    }

    public func toggleShuffle() async {
        do {
            let enabled = !isShuffleEnabled
            let snapshot = try await invoke { try $0.setShuffle(enabled: enabled) }
            apply(snapshot: snapshot)
            status = snapshot.shuffleEnabled ? "Shuffle on" : "Shuffle off"
        } catch {
            report(error)
        }
    }

    public func cycleRepeatMode() async {
        await setRepeatMode(repeatMode.next)
    }

    public func setRepeatMode(_ mode: PlaybackRepeatMode) async {
        do {
            let snapshot = try await invoke { try $0.setRepeatMode(mode) }
            apply(snapshot: snapshot)
            status = snapshot.repeatMode.label
        } catch {
            report(error)
        }
    }

    public func setSelectedFavorite(_ enabled: Bool = true) async {
        guard let track = selectedTrack ?? nowPlaying else {
            status = "Select a track first"
            return
        }

        await runBusy(enabled ? "Adding favorite" : "Removing favorite") { [self] in
            try await invoke { try $0.setFavorite(path: track.path, enabled: enabled) }
            status = enabled ? "Favorited \(track.title)" : "Removed favorite"
            if libraryScope == .favorites {
                await reloadActiveScope(quiet: true)
            }
        }
    }

    public func setRating(_ rating: Int?) async {
        guard let track = detailTrack else {
            status = "Select or play a track first"
            return
        }
        if let rating, !(1...10).contains(rating) {
            status = "Rating must be between 1 and 10"
            return
        }

        await runBusy("Updating rating") { [self] in
            let updated = try await invoke { try $0.setTrackRating(path: track.path, rating: rating) }
            replaceTrackView(updated)
            status = rating.map { "Rated \($0)/10" } ?? "Cleared rating"
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
            status = "Saved track cover"
            await reloadActiveScope(
                quiet: true,
                preferredSelectedViewID: updated.id,
                forceDetails: true,
                preferredSelectedView: updated
            )
            await refreshPlaylists()
        }
    }

    public func setTrackArtwork(_ imageURL: URL) async {
        guard let track = detailTrack else {
            status = "Select or play a track first"
            return
        }
        await setTrackArtwork(for: track, imageURL: imageURL)
    }

    public func setAlbumArtwork(for track: TrackItem, imageURL: URL) async {
        guard track.hasAlbumIdentity else {
            status = "Album metadata is required before setting an album cover"
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
            status = summary.tracksUpdated == 0
                ? "No tracks matched this album"
                : "Updated album cover for \(summary.tracksUpdated) tracks"
            await reloadActiveScope(
                quiet: true,
                preferredSelectedViewID: track.id,
                forceDetails: true
            )
            await refreshPlaylists()
        }
    }

    public func setAlbumArtwork(_ imageURL: URL) async {
        guard let track = detailTrack else {
            status = "Select or play a track first"
            return
        }
        await setAlbumArtwork(for: track, imageURL: imageURL)
    }

    public func presentViewEdit() {
        guard let track = detailTrack else {
            status = "Select or play a track first"
            return
        }
        let details = matchingDetails(for: track)
        viewEditNameDraft = details?.viewName ?? track.viewName ?? ""
        viewEditTitleDraft = details?.displayTitle ?? track.title
        viewEditArtistDraft = details?.displayArtist ?? track.artist
        viewEditAlbumDraft = details?.displayAlbum ?? track.album
        viewEditNotesDraft = details?.notes ?? ""
        viewEditArtworkURL = nil
        viewEditLyricsURL = nil
        isViewEditPresented = true
    }

    public func cancelViewEdit() {
        isViewEditPresented = false
        resetViewEditDrafts()
    }

    public func setViewEditArtworkURL(_ url: URL) {
        viewEditArtworkURL = url
    }

    public func setViewEditLyricsURL(_ url: URL) {
        viewEditLyricsURL = url
    }

    public func saveViewEdit() async {
        guard let track = detailTrack else {
            status = "Select or play a track first"
            return
        }
        let title = viewEditTitleDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else {
            status = "Title cannot be empty"
            return
        }

        let edit = TrackViewEdit(
            viewName: viewEditNameDraft.trimmingCharacters(in: .whitespacesAndNewlines),
            title: title,
            artist: viewEditArtistDraft.trimmingCharacters(in: .whitespacesAndNewlines),
            album: viewEditAlbumDraft.trimmingCharacters(in: .whitespacesAndNewlines),
            notes: viewEditNotesDraft,
            artworkPath: viewEditArtworkURL?.path,
            lyricsPath: viewEditLyricsURL?.path
        )

        isViewSaving = true
        await runBusy("Saving view") { [self] in
            let artworkAccessGranted = viewEditArtworkURL?.startAccessingSecurityScopedResource() ?? false
            let lyricsAccessGranted = viewEditLyricsURL?.startAccessingSecurityScopedResource() ?? false
            defer {
                if artworkAccessGranted {
                    viewEditArtworkURL?.stopAccessingSecurityScopedResource()
                }
                if lyricsAccessGranted {
                    viewEditLyricsURL?.stopAccessingSecurityScopedResource()
                }
            }
            let updated = try await invoke { try $0.editTrackView(path: track.path, edit: edit) }
            status = "Saved view"
            isViewEditPresented = false
            resetViewEditDrafts()
            await reloadActiveScope(
                quiet: true,
                preferredSelectedViewID: updated.id,
                forceDetails: true,
                preferredSelectedView: updated
            )
        }
        isViewSaving = false
    }

    public func materializeSelected(to destinationURL: URL) async {
        guard let track = detailTrack else {
            status = "Select or play a track first"
            return
        }

        await runBusy("Materializing view") { [self] in
            let materialized = try await invoke {
                try $0.exportTrackView(path: track.path, destinationURL: destinationURL)
            }
            status = "Exported \(materialized.title)"
            await reloadActiveScope(
                quiet: true,
                preferredSelectedViewID: materialized.id,
                forceDetails: true,
                preferredSelectedView: materialized
            )
        }
    }

    private func pollPlayback() async {
        guard !isPolling else {
            return
        }
        isPolling = true
        defer { isPolling = false }

        do {
            let snapshot = try await invoke { try $0.poll() }
            apply(snapshot: snapshot)
        } catch {
            report(error)
        }
    }

    #if os(macOS)
    private func handleLibraryWorker(_ event: LibraryWorkerEvent) {
        switch event.name {
        case "started":
            let total = event.total ?? 0
            libraryProgress = total == 0 ? 1 : 0
            libraryStatus = total == 0 ? "\(event.operation.capitalized) found no tracks" : "\(event.operation.capitalized) 0 / \(total)"
            status = libraryStatus
        case "track_started":
            if let index = event.index, let total = event.total, total > 0 {
                libraryProgress = Double(max(0, index - 1)) / Double(total)
                libraryStatus = "\(event.operation.capitalized) \(index) / \(total): \(event.title ?? "track")"
            }
        case "track_finished", "track_skipped":
            if let index = event.index, let total = event.total, total > 0 {
                libraryProgress = Double(index) / Double(total)
                libraryStatus = "\(event.operation.capitalized) \(index) / \(total): \(event.title ?? "track")"
            }
            if event.operation == "import" {
                playbackDetail = "Imported \(event.imported ?? 0), copied \(event.copied ?? 0), duplicates \(event.duplicatesSkipped ?? 0), artwork \(event.artworkCached ?? 0), warnings \(event.metadataWarnings ?? 0), failures \(event.failures ?? 0)"
            } else {
                playbackDetail = "Audit failures \(event.failures ?? 0)"
            }
        case "track_failed":
            playbackError = event.error ?? "Library worker track failed"
            if let index = event.index, let total = event.total, total > 0 {
                libraryProgress = Double(index) / Double(total)
            }
        case "merge_finished":
            playbackDetail = "Audit groups \(event.duplicateGroups ?? 0), merged \(event.tracksMerged ?? 0), failures \(event.failures ?? 0)"
        case "finished":
            isLibraryWorking = false
            isBusy = false
            libraryProgress = 1
            if event.operation == "import" {
                libraryStatus = "Import finished"
                status = "Import finished"
                playbackDetail = "Imported \(event.imported ?? 0), copied \(event.copied ?? 0), duplicates \(event.duplicatesSkipped ?? 0), artwork \(event.artworkCached ?? 0), warnings \(event.metadataWarnings ?? 0), failures \(event.failures ?? 0)"
            } else {
                libraryStatus = "Audit finished"
                status = "Audit finished"
                playbackDetail = "Audit scanned \(event.tracksScanned ?? 0), hashes \(event.hashesUpdated ?? 0), groups \(event.duplicateGroups ?? 0), merged \(event.tracksMerged ?? 0), failures \(event.failures ?? 0)"
            }
            Task {
                await reloadActiveScope(quiet: true)
                await refreshPlaylists()
            }
        case "fatal", "stderr", "decode_error":
            playbackError = event.error ?? "Library worker error"
            status = "Library worker error"
        default:
            break
        }
    }

    private func handleLibraryWorkerExit(_ exitCode: Int32) async {
        libraryWorker = nil
        if exitCode == 0 {
            isLibraryWorking = false
            isBusy = false
            if libraryProgress != 1 {
                libraryProgress = 1
                libraryStatus = "Library task finished"
                status = "Library task finished"
                await reloadActiveScope(quiet: true)
                await refreshPlaylists()
            }
        } else if isLibraryWorking {
            isLibraryWorking = false
            isBusy = false
            libraryProgress = nil
            status = "Library task stopped"
            playbackError = "Library worker exited with code \(exitCode)"
        }
    }
    #endif

    #if os(macOS)
    private func handleAnalyzer(_ event: AnalyzerWorkerEvent) {
        switch event.name {
        case "started":
            let total = event.total ?? 0
            analyzeProgress = total == 0 ? 1 : 0
            analyzeStatus = total == 0 ? "No tracks need analysis" : "Analyzing 0 / \(total)"
            status = total == 0 ? "Analysis finished" : "Analyzing in background"
        case "track_started":
            if let index = event.index, let total = event.total, total > 0 {
                analyzeProgress = Double(max(0, index - 1)) / Double(total)
                analyzeStatus = "Analyzing \(index) / \(total): \(event.title ?? "track")"
            }
        case "track_finished":
            if let index = event.index, let total = event.total, total > 0 {
                analyzeProgress = Double(index) / Double(total)
                analyzeStatus = "Analyzed \(index) / \(total): \(event.title ?? "track")"
                playbackDetail = "Loudness cache updated: \(event.analyzed ?? index) ok, \(event.failed ?? 0) failed"
            }
        case "track_failed":
            if let index = event.index, let total = event.total, total > 0 {
                analyzeProgress = Double(index) / Double(total)
                analyzeStatus = "Analyze failed \(index) / \(total): \(event.title ?? "track")"
            }
            playbackError = event.error ?? "Track analysis failed"
        case "album_finished":
            playbackDetail = "Album loudness: \(event.albumsAnalyzed ?? 0) albums, \(event.albumTracksUpdated ?? 0) tracks"
        case "finished":
            isAnalyzing = false
            analyzeProgress = 1
            analyzeStatus = "Analysis finished: \(event.analyzed ?? 0) ok, \(event.failed ?? 0) failed"
            status = "Analysis finished"
            playbackDetail = "Tracks \(event.analyzed ?? 0), albums \(event.albumsAnalyzed ?? 0), failures \(event.failed ?? 0)"
            Task {
                await reloadActiveScope(quiet: true)
            }
        case "fatal", "stderr", "decode_error":
            playbackError = event.error ?? "Analyzer worker error"
            status = "Analyzer error"
        default:
            break
        }
    }

    private func handleAnalyzerExit(_ exitCode: Int32) async {
        analyzerWorker = nil
        if exitCode == 0 {
            isAnalyzing = false
            if analyzeProgress != 1 {
                analyzeProgress = 1
                analyzeStatus = "Analysis finished"
                status = "Analysis finished"
                await reloadActiveScope(quiet: true)
            }
        } else if isAnalyzing {
            isAnalyzing = false
            analyzeProgress = nil
            status = "Analyzer stopped"
            playbackError = "Analyzer exited with code \(exitCode)"
        }
    }
    #endif

    private func applyLoadedViews(_ loaded: [TrackItem], preferredSelectedViewID: String?) {
        allTrackViews = loaded

        if let preferredSelectedViewID,
           let preferred = loaded.first(where: { $0.id == preferredSelectedViewID }) {
            setActiveView(preferred)
            selectedTrack = preferred
        } else if let selectedTrack,
                  let refreshed = loaded.first(where: { $0.id == selectedTrack.id }) {
            setActiveView(refreshed)
            self.selectedTrack = refreshed
        } else if let selectedTrack,
                  let fallback = loaded.first(where: { $0.primaryViewID == selectedTrack.primaryViewID }) {
            setActiveView(fallback)
            self.selectedTrack = fallback
        } else {
            selectedTrack = nil
        }

        if let nowPlaying,
           let refreshed = loaded.first(where: { $0.id == nowPlaying.id }) {
            self.nowPlaying = refreshed
            setActiveView(refreshed)
        }

        pruneActiveViews(to: loaded)
        tracks = visibleTracks(from: loaded)
    }

    private func collapsedTracks(from views: [TrackItem]) -> [TrackItem] {
        var groups: [String: [TrackItem]] = [:]
        var primaryOrder: [String] = []
        for view in views {
            let primaryID = view.primaryViewID
            if groups[primaryID] == nil {
                primaryOrder.append(primaryID)
            }
            groups[primaryID, default: []].append(view)
        }

        return primaryOrder.compactMap { primaryID in
            guard let options = groups[primaryID] else {
                return nil
            }
            if let activeID = activeViewIDByPrimaryID[primaryID],
               let active = options.first(where: { $0.id == activeID }) {
                return active
            }
            if let primary = options.first(where: { $0.isPrimaryView }) {
                activeViewIDByPrimaryID[primaryID] = primary.id
                return primary
            }
            let fallback = options[0]
            activeViewIDByPrimaryID[primaryID] = fallback.id
            return fallback
        }
    }

    private func visibleTracks(from views: [TrackItem]) -> [TrackItem] {
        sortedTrackItems(collapsedTracks(from: views), by: playlistSortMode)
    }

    private func sortedTrackItems(_ items: [TrackItem], by sortMode: PlaylistSortMode) -> [TrackItem] {
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

    private func compareSortKeys(_ left: [String], _ right: [String]) -> Bool {
        for (leftValue, rightValue) in zip(left, right) {
            let comparison = leftValue.localizedStandardCompare(rightValue)
            if comparison != .orderedSame {
                return comparison == .orderedAscending
            }
        }
        return false
    }

    private func sortValue(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "\u{10FFFF}" : trimmed
    }

    private func replaceTrackView(_ updated: TrackItem) {
        if let index = allTrackViews.firstIndex(where: { $0.id == updated.id }) {
            allTrackViews[index] = updated
        } else {
            allTrackViews.append(updated)
        }

        setActiveView(updated)
        if selectedTrack?.id == updated.id || selectedTrack?.path == updated.path {
            selectedTrack = updated
        }
        if nowPlaying?.id == updated.id || nowPlaying?.path == updated.path {
            nowPlaying = updated
        }
        tracks = visibleTracks(from: allTrackViews)
    }

    private func viewChoices(for track: TrackItem) -> [TrackViewChoice] {
        let views = allTrackViews.filter { $0.primaryViewID == track.primaryViewID }
        let options = views.isEmpty ? [track] : views
        return options.enumerated().map { index, option in
            TrackViewChoice(id: option.id, track: option, index: index, total: options.count)
        }
    }

    private func setActiveView(_ view: TrackItem) {
        activeViewIDByPrimaryID[view.primaryViewID] = view.id
    }

    private func pruneActiveViews(to views: [TrackItem]) {
        let primaryIDs = Set(views.map(\.primaryViewID))
        activeViewIDByPrimaryID = activeViewIDByPrimaryID.filter { primaryIDs.contains($0.key) }
    }

    private func apply(snapshot: PlaybackSnapshot, fallbackTrack: TrackItem? = nil) {
        let previousTrackID = nowPlaying?.id
        playbackError = snapshot.error ?? ""
        playbackElapsedMS = snapshot.positionMS
        isPlaying = snapshot.isPlaying
        isAudioInterrupted = snapshot.interruptionActive
        resumeAfterAudioInterruption = snapshot.resumeAfterInterruption
        repeatMode = snapshot.repeatMode
        isShuffleEnabled = snapshot.shuffleEnabled
        queueCount = snapshot.queueLen
        queuePosition = snapshot.queuePosition

        if let track = snapshot.currentTrack ?? fallbackTrack {
            nowPlaying = track
            setActiveView(track)
            if !allTrackViews.isEmpty {
                tracks = visibleTracks(from: allTrackViews)
            }
            if previousTrackID != track.id {
                let shouldFollowNowPlaying = selectedTrack == nil || selectedTrack?.id == previousTrackID
                if shouldFollowNowPlaying {
                    selectedTrack = allTrackViews.first(where: { $0.id == track.id }) ?? track
                }
            }
            if detailTrack?.id == track.id
                && (previousTrackID != track.id || (nowPlayingDetails == nil && !isLoadingDetails)) {
                loadDetails(for: track)
            }
        } else if !snapshot.isPlaying {
            nowPlaying = nil
            if let selectedTrack {
                loadDetails(for: selectedTrack)
            } else {
                clearDetails()
            }
            playbackElapsedMS = 0
        }

        if let gainDB = snapshot.gainDB {
            playbackDetail = String(format: "Normalize gain %@ dB", String(format: "%+.1f", gainDB))
        } else if let loudnessStatus = snapshot.loudnessStatus {
            playbackDetail = loudnessStatus
        }

        if let error = snapshot.error, !error.isEmpty {
            status = "Playback error"
            playbackError = error
        }

        if nowPlaying == nil {
            if previousTrackID != nil {
                playbackSystemIntegration?.playbackDidStop()
            }
            stopPlaybackTimer()
        } else {
            startPlaybackTimer()
        }
    }

    private func loadDetails(for track: TrackItem, force: Bool = false) {
        if !force {
            if loadingDetailsTrackID == track.id {
                return
            }
            if detailsTrackID == track.id && nowPlayingDetails != nil {
                return
            }
        }

        detailsTask?.cancel()
        if detailsTrackID != track.id {
            nowPlayingDetails = TrackDetails.placeholder(for: track)
            detailsTrackID = track.id
        }
        loadingDetailsTrackID = track.id
        isLoadingDetails = true

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
                    self.nowPlayingDetails = details
                    self.detailsTrackID = track.id
                    self.loadingDetailsTrackID = nil
                    self.isLoadingDetails = false
                }
            } catch {
                guard !Task.isCancelled else {
                    return
                }
                if self.detailTrack?.id == track.id {
                    self.loadingDetailsTrackID = nil
                    self.isLoadingDetails = false
                    self.playbackDetail = "Details unavailable: \(error.localizedDescription)"
                }
            }
        }
    }

    private func clearDetails() {
        detailsTask?.cancel()
        detailsTask = nil
        detailsTrackID = nil
        loadingDetailsTrackID = nil
        nowPlayingDetails = nil
        resetViewEditDrafts()
        isLoadingDetails = false
    }

    private func resetViewEditDrafts() {
        viewEditNameDraft = ""
        viewEditTitleDraft = ""
        viewEditArtistDraft = ""
        viewEditAlbumDraft = ""
        viewEditNotesDraft = ""
        viewEditArtworkURL = nil
        viewEditLyricsURL = nil
        isViewSaving = false
    }

    private func startPlaybackTimer() {
        guard playbackTimer == nil else {
            return
        }
        playbackTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor in
                await self?.pollPlayback()
            }
        }
    }

    private func stopPlaybackTimer() {
        playbackTimer?.invalidate()
        playbackTimer = nil
    }

    private func normalizedDraft(_ value: String?) -> String {
        value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }

    private func matchingDetails(for track: TrackItem) -> TrackDetails? {
        guard let details = nowPlayingDetails,
              details.viewID == track.viewID else {
            return nil
        }
        return details
    }

    private func defaultNewPlaylistName() -> String {
        let baseName = "New Playlist"
        let existingNames = Set(playlists.map(\.name))
        if !existingNames.contains(baseName) {
            return baseName
        }
        var index = 2
        while existingNames.contains("\(baseName) \(index)") {
            index += 1
        }
        return "\(baseName) \(index)"
    }

    private func clearPlaylistSettingsDraft() {
        playlistSettingsOriginalName = nil
        playlistSettingsNameDraft = ""
        playlistSettingsArtworkURL = nil
        playlistSettingsCurrentArtworkURL = nil
    }

    private func runBusy(_ busyStatus: String?, operation: () async throws -> Void) async {
        if let busyStatus {
            status = busyStatus
        }
        isBusy = true
        defer { isBusy = false }

        do {
            try await operation()
        } catch {
            report(error)
        }
    }

    private func invoke<T: Sendable>(_ operation: @escaping @Sendable (RustPlayerClient) throws -> T) async throws -> T {
        let client = self.client
        return try await Task.detached(priority: .userInitiated) {
            try operation(client)
        }.value
    }

    private func report(_ error: Error) {
        status = "Error"
        playbackError = error.localizedDescription
        writeImportDebugLog("error: \(error.localizedDescription)")
    }

    private func writeImportDebugLog(_ message: String) {
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

    private func formatTime(_ milliseconds: Int) -> String {
        let totalSeconds = max(0, milliseconds / 1000)
        return "\(totalSeconds / 60):\(String(format: "%02d", totalSeconds % 60))"
    }
}
