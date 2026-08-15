import Combine
import Foundation

public enum PlaylistSheetDestination: Hashable, Sendable {
    case create
    case picker
    case settings
}

@MainActor
public final class LibraryFeatureState: ObservableObject {
    @Published public internal(set) var tracks: [TrackItem] = []
    @Published public internal(set) var selectedTrack: TrackItem?
    @Published public internal(set) var query = ""
    @Published public internal(set) var scope: LibraryScope = .library
}

@MainActor
public final class PlaylistFeatureState: ObservableObject {
    @Published public internal(set) var items: [PlaylistItem] = []
    @Published public internal(set) var recentItems: [PlaylistItem] = []
    @Published public internal(set) var presentedSheet: PlaylistSheetDestination?
    @Published public internal(set) var newNameDraft = "New Playlist"
    @Published public internal(set) var settingsOriginalName: String?
    @Published public internal(set) var settingsNameDraft = ""
    @Published public internal(set) var settingsArtworkURL: URL?
    @Published public internal(set) var settingsCurrentArtworkURL: URL?
    @Published public internal(set) var sortMode: PlaylistSortMode = .defaultOrder
    @Published public internal(set) var pickerTrack: TrackItem?
    internal var addsPickerTrackAfterCreate = false
}

@MainActor
public final class PlaybackFeatureState: ObservableObject {
    @Published public internal(set) var queue: [TrackItem] = []
    @Published public internal(set) var isPlaying = false
    @Published public internal(set) var isAudioInterrupted = false
    @Published public internal(set) var nowPlaying: TrackItem?
    @Published public internal(set) var elapsedMS = 0
    @Published public internal(set) var error = ""
    @Published public internal(set) var detail = ""
    @Published public internal(set) var repeatMode: PlaybackRepeatMode = .off
    @Published public internal(set) var isShuffleEnabled = false
    @Published public internal(set) var queueCount = 0
    @Published public internal(set) var queuePosition: Int?
    @Published public internal(set) var details: TrackDetails?
    @Published public internal(set) var isLoadingDetails = false
}

@MainActor
public final class TrackDetailFeatureState: ObservableObject {
    @Published public internal(set) var details: TrackDetails?
    @Published public internal(set) var isLoading = false
    @Published public internal(set) var isEditPresented = false
    @Published public internal(set) var isSaving = false
    @Published public internal(set) var titleDraft = ""
    @Published public internal(set) var artistDraft = ""
    @Published public internal(set) var albumDraft = ""
    @Published public internal(set) var notesDraft = ""
    @Published public internal(set) var artworkURL: URL?
    @Published public internal(set) var lyricsURL: URL?
}

@MainActor
public final class OperationFeatureState: ObservableObject {
    @Published public internal(set) var status = "Ready"
    @Published public internal(set) var isBusy = false
    @Published public internal(set) var isAnalyzing = false
    @Published public internal(set) var analyzeProgress: Double?
    @Published public internal(set) var analyzeStatus = ""
    @Published public internal(set) var isLibraryWorking = false
    @Published public internal(set) var libraryProgress: Double?
    @Published public internal(set) var libraryStatus = ""
    @Published public internal(set) var lastLibraryBackupURL: URL?
    @Published public internal(set) var startupError: String?
}
