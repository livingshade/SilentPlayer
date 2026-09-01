import Combine
import Foundation

@MainActor
public final class AppModel: ObservableObject {
    public let library = LibraryFeatureState()
    public let playlists = PlaylistFeatureState()
    public let playback = PlaybackFeatureState()
    public let trackDetail = TrackDetailFeatureState()
    public let operations = OperationFeatureState()
    @Published public internal(set) var discordPresenceStatus = "Off"
    @Published public internal(set) var isDiscordPresenceSharing = false

    private var featureStateSubscriptions: Set<AnyCancellable> = []

    public var startupError: String? { operations.startupError }

    internal let client: RustPlayerClient?
    internal var playbackSystemIntegration: (any PlaybackSystemIntegration)?
    internal var discordPresenceIntegration: (any PlaybackSystemIntegration)?
    internal var resumeAfterAudioInterruption = false
    nonisolated(unsafe) internal var playbackTimer: Timer?
    internal var isPolling = false
    #if os(macOS)
    internal var analyzerWorker: AnalyzerWorker?
    internal var libraryWorker: LibraryWorker?
    #endif
    internal var detailsTask: Task<Void, Never>?
    internal var playbackDetailsTask: Task<Void, Never>?
    internal var trackEditTarget: TrackItem?
    internal var detailsTrackID: String?
    internal var loadingDetailsTrackID: String?
    internal var playbackDetailsTrackID: String?
    internal var playbackPositionReferenceUptime = ProcessInfo.processInfo.systemUptime
    internal var loadedTracks: [TrackItem] = []
    internal var libraryPresentationCache: LibraryPresentationCache?
    internal var isPresentingCompleteLibrary = false
    internal var hasBootstrapped = false

    public init(
        client: RustPlayerClient? = nil,
        discoverClient: () throws -> RustPlayerClient = RustPlayerClient.discover
    ) {
        if let client {
            self.client = client
        } else {
            do {
                self.client = try discoverClient()
            } catch {
                self.client = nil
                let message = "Unable to start the player service: \(error.localizedDescription)"
                operations.startupError = message
                playback.error = message
                operations.status = "Player unavailable"
            }
        }
        forwardFeatureStateChanges()
    }

    private func forwardFeatureStateChanges() {
        [
            library.objectWillChange.eraseToAnyPublisher(),
            playlists.objectWillChange.eraseToAnyPublisher(),
            playback.objectWillChange.eraseToAnyPublisher(),
            trackDetail.objectWillChange.eraseToAnyPublisher(),
            operations.objectWillChange.eraseToAnyPublisher()
        ]
        .forEach { publisher in
            publisher
                .sink { [weak self] _ in self?.objectWillChange.send() }
                .store(in: &featureStateSubscriptions)
        }
    }

    deinit {
        playbackTimer?.invalidate()
        #if os(macOS)
        analyzerWorker?.stop()
        libraryWorker?.stop()
        #endif
        detailsTask?.cancel()
        playbackDetailsTask?.cancel()
    }
}
