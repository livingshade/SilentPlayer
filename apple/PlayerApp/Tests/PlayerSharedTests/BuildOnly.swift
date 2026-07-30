import Combine
import Foundation
import XCTest
@testable import PlayerShared

#if os(iOS)
import MediaPlayer
import UIKit

private final class SendableArtworkBox: @unchecked Sendable {
    let artwork: MPMediaItemArtwork

    init(artwork: MPMediaItemArtwork) {
        self.artwork = artwork
    }
}
#endif

let playerSharedTestsBuildAnchor = TrackItem(
    id: "audio:test-anchor",
    title: "Anchor",
    artist: "Artist",
    durationMS: nil,
    path: "/tmp/anchor.wav"
)

final class PlaybackPolicyTests: XCTestCase {
    func testWireEnumsUseOnlyCanonicalValues() throws {
        XCTAssertEqual(PlaylistSortMode.defaultOrder.apiValue, "manual")
        XCTAssertEqual(
            try JSONDecoder().decode(PlaybackRepeatMode.self, from: Data(#""all""#.utf8)).rawValue,
            "all"
        )
        XCTAssertThrowsError(
            try JSONDecoder().decode(PlaybackRepeatMode.self, from: Data(#""loop""#.utf8))
        )
    }

    func testInterruptionOnlyPreparesWhenBothSystemAndLifecycleRequestResume() {
        XCTAssertTrue(PlaybackInterruptionPolicy.shouldPrepareForResume(
            systemShouldResume: true,
            resumeWasScheduled: true
        ))
        XCTAssertFalse(PlaybackInterruptionPolicy.shouldPrepareForResume(
            systemShouldResume: true,
            resumeWasScheduled: false
        ))
        XCTAssertFalse(PlaybackInterruptionPolicy.shouldPrepareForResume(
            systemShouldResume: false,
            resumeWasScheduled: true
        ))
    }

    func testRouteChangeOnlyPausesForRemovedPrivateOutput() {
        XCTAssertTrue(PlaybackRouteChangePolicy.shouldPause(
            oldDeviceBecameUnavailable: true,
            previousRouteHadPrivateOutput: true
        ))
        XCTAssertFalse(PlaybackRouteChangePolicy.shouldPause(
            oldDeviceBecameUnavailable: true,
            previousRouteHadPrivateOutput: false
        ))
        XCTAssertFalse(PlaybackRouteChangePolicy.shouldPause(
            oldDeviceBecameUnavailable: false,
            previousRouteHadPrivateOutput: true
        ))
    }

    func testRemotePlayCommandsAreDisabledDuringAnInterruption() {
        XCTAssertTrue(PlaybackRemoteCommandPolicy.canPlay(
            hasTrack: true,
            isInterrupted: false
        ))
        XCTAssertFalse(PlaybackRemoteCommandPolicy.canPlay(
            hasTrack: true,
            isInterrupted: true
        ))
        XCTAssertFalse(PlaybackRemoteCommandPolicy.canPlay(
            hasTrack: false,
            isInterrupted: false
        ))
    }

    func testLockScreenPlaybackCommandsStayAvailableAndRateRepresentsState() {
        XCTAssertTrue(PlaybackRemoteCommandPolicy.canPlay(
            hasTrack: true,
            isInterrupted: false
        ))
        XCTAssertTrue(PlaybackRemoteCommandPolicy.canPause(
            hasTrack: true,
            isInterrupted: false
        ))
        XCTAssertTrue(PlaybackRemoteCommandPolicy.canTogglePlayPause(
            hasTrack: true,
            isInterrupted: false
        ))
        XCTAssertEqual(PlaybackNowPlayingPolicy.playbackRate(isPlaying: true), 1)
        XCTAssertEqual(PlaybackNowPlayingPolicy.playbackRate(isPlaying: false), 0)
    }

    func testPlaybackPollingOnlyRunsWhileActivelyPlaying() {
        XCTAssertTrue(PlaybackPollingPolicy.shouldPoll(
            hasNowPlayingItem: true,
            isPlaying: true
        ))
        XCTAssertFalse(PlaybackPollingPolicy.shouldPoll(
            hasNowPlayingItem: true,
            isPlaying: false
        ))
        XCTAssertFalse(PlaybackPollingPolicy.shouldPoll(
            hasNowPlayingItem: false,
            isPlaying: true
        ))
        XCTAssertGreaterThan(PlaybackPollingPolicy.timerTolerance, 0)
        XCTAssertLessThan(
            PlaybackPollingPolicy.timerTolerance,
            PlaybackPollingPolicy.timerInterval
        )
    }

    @MainActor
    func testEnteringBackgroundKeepsActivePlaybackSessionPrepared() {
        let model = AppModel(discoverClient: {
            throw RustPlayerError.startupFailed("background playback test")
        })
        let integration = RecordingPlaybackSystemIntegration()
        model.installPlaybackSystemIntegration(integration)

        model.applicationDidEnterBackground()
        XCTAssertEqual(integration.backgroundCount, 0)

        model.isPlaying = true
        model.applicationDidEnterBackground()
        XCTAssertEqual(integration.backgroundCount, 1)
    }

    func testTrackChangeStatusMatchesConfirmedPlaybackState() {
        XCTAssertEqual(
            PlaybackStatusText.afterTrackChange(isPlaying: true, title: "Next Track"),
            "Playing Next Track"
        )
        XCTAssertEqual(
            PlaybackStatusText.afterTrackChange(isPlaying: false, title: "Next Track"),
            "Paused at Next Track"
        )
        XCTAssertEqual(
            PlaybackStatusText.afterTrackChange(isPlaying: false, title: "   "),
            "Paused at track"
        )
    }
}

@MainActor
final class PlaybackNowPlayingObservationTests: XCTestCase {
    func testPeriodicProgressAndDuplicateAssignmentsDoNotRequestPublication() {
        let model = AppModel(discoverClient: {
            throw RustPlayerError.startupFailed("observation test")
        })
        var observedStates: [PlaybackNowPlayingObservedState] = []
        let observation = PlaybackNowPlayingObservation.publisher(for: model)
            .sink { observedStates.append($0) }

        XCTAssertEqual(observedStates.count, 1)

        model.playbackElapsedMS = 500
        model.playbackElapsedMS = 1_000
        model.isPlaying = false
        model.nowPlaying = nil

        XCTAssertEqual(observedStates.count, 1)

        model.nowPlaying = playerSharedTestsBuildAnchor
        XCTAssertEqual(observedStates.count, 2)

        model.isPlaying = true
        XCTAssertEqual(observedStates.count, 3)

        withExtendedLifetime(observation) {}
    }
}

#if os(iOS)
@MainActor
final class IOSNowPlayingArtworkFactoryTests: XCTestCase {
    func testRequestHandlerCanRunOutsideMainActor() async throws {
        let image = try XCTUnwrap(UIImage(systemName: "music.note"))
        let artworkBox = SendableArtworkBox(
            artwork: IOSNowPlayingArtworkFactory.make(image: image)
        )

        let returnedImage = await Task.detached {
            artworkBox.artwork.image(at: CGSize(width: 32, height: 32)) != nil
        }.value

        XCTAssertTrue(returnedImage)
    }
}
#endif

final class PhoneDisplayTextTests: XCTestCase {
    func testCollapsesImportedLineBreaksAndWhitespace() {
        XCTAssertEqual(
            PhoneDisplayText.compact("  A title\nwith\tmetadata   spacing  "),
            "A title with metadata spacing"
        )
    }
}

final class PhonePresentationStateTests: XCTestCase {
    func testTabsRepresentOnlyStableTopLevelDestinations() {
        XCTAssertEqual(PhonePresentationTab.allCases, [.library, .playlists])
    }

    func testSnapshotRoundTripsThroughSceneStorageEncoding() throws {
        let snapshot = PhonePresentationSnapshot(
            selectedTab: .playlists,
            contentScope: .playlist(42),
            playlistDetailID: 42,
            selectedTrackID: "track:favorite"
        )

        let encoded = try XCTUnwrap(PhonePresentationPersistence.encode(snapshot))

        XCTAssertEqual(PhonePresentationPersistence.decode(encoded), snapshot)
        XCTAssertEqual(snapshot.bootstrapScope, .playlist(42))
    }

    func testDeletedPlaylistFallsBackToLibraryAndClearsDetailRoute() {
        let snapshot = PhonePresentationSnapshot(
            selectedTab: .playlists,
            contentScope: .playlist(42),
            playlistDetailID: 42,
            selectedTrackID: nil
        )

        let validated = snapshot.validated(against: [])

        XCTAssertEqual(validated.contentScope, .library)
        XCTAssertNil(validated.playlistDetailID)
    }

}

final class MacPresentationStateTests: XCTestCase {
    func testSnapshotRoundTripsThroughSceneStorageEncoding() throws {
        let snapshot = MacPresentationSnapshot(
            contentScope: .playlist(73),
            selectedTrackID: "track:studio"
        )

        let encoded = try XCTUnwrap(MacPresentationPersistence.encode(snapshot))

        XCTAssertEqual(MacPresentationPersistence.decode(encoded), snapshot)
    }

    func testDeletedPlaylistFallsBackToLibrary() {
        let snapshot = MacPresentationSnapshot(
            contentScope: .playlist(73),
            selectedTrackID: "track:studio"
        )

        let validated = snapshot.validated(against: [])

        XCTAssertEqual(validated.contentScope, .library)
        XCTAssertEqual(validated.selectedTrackID, "track:studio")
    }

}

@MainActor
final class AppModelStartupTests: XCTestCase {
    func testStartupFailureBecomesVisibleStateInsteadOfCrashing() async {
        let model = AppModel(discoverClient: {
            throw RustPlayerError.startupFailed("test startup failure")
        })

        XCTAssertEqual(model.status, "Player unavailable")
        XCTAssertEqual(
            model.startupError,
            "Unable to start the player service: test startup failure"
        )
        XCTAssertEqual(model.playbackError, model.startupError)

        await model.bootstrap()

        XCTAssertEqual(model.status, "Player unavailable")
        XCTAssertEqual(model.playbackError, model.startupError)
        XCTAssertTrue(model.tracks.isEmpty)
    }
}

@MainActor
final class AppModelPresentationRestorationTests: XCTestCase {
    func testBootstrapRestoresPlaylistByStableID() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        try client.createPlaylist(name: "Road Trip")
        let playlist = try XCTUnwrap(client.playlists().first)
        let model = AppModel(client: client)

        await model.bootstrap(restoring: .playlist(playlist.id))

        XCTAssertEqual(model.libraryScope, .playlist("Road Trip"))
        XCTAssertEqual(model.restorableLibraryScope, .playlist(playlist.id))
    }

    func testBootstrapFallsBackWhenRestoredPlaylistWasDeleted() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        let model = AppModel(client: client)

        await model.bootstrap(restoring: .playlist(Int64.max))

        XCTAssertEqual(model.libraryScope, .library)
        XCTAssertEqual(model.restorableLibraryScope, .library)
    }

    func testPlaylistSearchKeepsTheActivePlaylistScope() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        try client.createPlaylist(name: "Road Trip")
        let playlist = try XCTUnwrap(client.playlists().first)
        let model = AppModel(client: client)
        await model.bootstrap(restoring: .playlist(playlist.id))

        model.query = "missing"
        await model.search()

        XCTAssertEqual(model.libraryScope, .playlist("Road Trip"))
        XCTAssertEqual(model.status, "No songs found in Road Trip")
    }
}

@MainActor
final class LibraryPresentationCacheTests: XCTestCase {
    func testReturningToLibraryUsesSessionCacheUntilExplicitRefresh() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        let model = AppModel(client: client)
        var loadingMessages: [String] = []
        let observation = model.$libraryStatus
            .filter { !$0.isEmpty }
            .sink { loadingMessages.append($0) }

        await model.bootstrap()
        let messagesAfterFirstLoad = loadingMessages.count
        XCTAssertGreaterThan(messagesAfterFirstLoad, 0)

        await model.bootstrap()
        XCTAssertEqual(loadingMessages.count, messagesAfterFirstLoad)

        await model.showFavorites()
        await model.showLibrary()
        XCTAssertEqual(model.libraryScope, .library)
        XCTAssertEqual(loadingMessages.count, messagesAfterFirstLoad)

        await model.refreshLibrary()
        XCTAssertGreaterThan(loadingMessages.count, messagesAfterFirstLoad)
        withExtendedLifetime(observation) {}
    }
}

@MainActor
final class SinglePrimaryPresentationTests: XCTestCase {
    func testLibraryDisplaysEveryTrackReturnedByRustWithoutClientSideCollapse() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        _ = try client.importFiles([
            repositoryRoot
                .appendingPathComponent("test-assets/audio/into_the_oceans_chorus.ogg"),
            repositoryRoot
                .appendingPathComponent("test-assets/audio/funk_room_reverb.ogg")
        ])
        let rustTracks = try client.library()
        let model = AppModel(client: client)

        await model.bootstrap()

        XCTAssertEqual(model.tracks.count, rustTracks.count)
        XCTAssertEqual(Set(model.tracks.map(\.id)), Set(rustTracks.map(\.id)))
    }

    func testMetadataEditReplacesTheSameTrackInPlace() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        _ = try client.importFiles([
            repositoryRoot
                .appendingPathComponent("test-assets/audio/into_the_oceans_chorus.ogg")
        ])
        let model = AppModel(client: client)
        await model.bootstrap()
        let original = try XCTUnwrap(model.tracks.first)
        model.selectTrack(id: original.id)
        model.presentTrackEdit()
        model.trackEditTitleDraft = "Updated in place"

        await model.saveTrackEdit()

        let updated = try XCTUnwrap(model.selectedTrack)
        XCTAssertEqual(model.tracks.count, 1)
        XCTAssertEqual(updated.id, original.id)
        XCTAssertEqual(updated.path, original.path)
        XCTAssertEqual(updated.title, "Updated in place")
        XCTAssertEqual(try client.library().count, 1)
    }

    func testRemovingFromPlaylistKeepsSongWhileDeletingFromLibraryRemovesItEverywhere() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        _ = try client.importFiles([
            repositoryRoot
                .appendingPathComponent("test-assets/audio/into_the_oceans_chorus.ogg"),
            repositoryRoot
                .appendingPathComponent("test-assets/audio/funk_room_reverb.ogg")
        ])
        try client.createPlaylist(name: "Road")
        let playlist = try XCTUnwrap(client.playlists().first)
        let target = try XCTUnwrap(client.library().first)
        try client.addToPlaylist(name: playlist.name, path: target.path)
        let model = AppModel(client: client)
        await model.bootstrap(restoring: .playlist(playlist.id))

        await model.removeFromActivePlaylist(target)

        XCTAssertEqual(try client.library().count, 2)
        XCTAssertTrue(try client.playlistTracks(name: playlist.name).isEmpty)
        try client.addToPlaylist(name: playlist.name, path: target.path)
        XCTAssertTrue(FileManager.default.fileExists(atPath: target.path))

        await model.deleteFromLibrary(target)

        XCTAssertEqual(try client.library().count, 1)
        XCTAssertTrue(try client.playlistTracks(name: playlist.name).isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: target.path))
        XCTAssertFalse(model.tracks.contains(where: { $0.id == target.id }))
        XCTAssertEqual(model.status, "Deleted \(target.title) from Library")
    }
}

private let repositoryRoot = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()

@MainActor
final class LibraryMigrationTests: XCTestCase {
    func testImportCreatesCompleteBackupBeforeReplacingLibrary() async throws {
        let container = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: container) }

        let sourceRoot = container.appendingPathComponent("Source", isDirectory: true)
        let sourceClient = try RustPlayerClient(
            dbURL: sourceRoot.appendingPathComponent("library.sqlite3"),
            mediaRootURL: sourceRoot.appendingPathComponent("Music", isDirectory: true),
            repoRoot: container
        )
        let importPackage = container
            .appendingPathComponent("Import.silentlibrary", isDirectory: true)
        _ = try sourceClient.exportLibrary(to: importPackage)

        let targetRoot = container.appendingPathComponent("Target", isDirectory: true)
        let targetClient = try RustPlayerClient(
            dbURL: targetRoot.appendingPathComponent("library.sqlite3"),
            mediaRootURL: targetRoot.appendingPathComponent("Music", isDirectory: true),
            repoRoot: container
        )
        let model = AppModel(client: targetClient)

        await model.importLibrary(from: importPackage)

        XCTAssertEqual(model.status, "Library imported")
        let backupURL = try XCTUnwrap(model.lastLibraryBackupURL)
        XCTAssertEqual(
            backupURL.deletingLastPathComponent(),
            targetRoot.appendingPathComponent("Backups", isDirectory: true)
        )
        XCTAssertTrue(
            FileManager.default.fileExists(
                atPath: backupURL.appendingPathComponent("manifest.json").path
            )
        )
        XCTAssertTrue(
            FileManager.default.fileExists(
                atPath: backupURL.appendingPathComponent("player_library.sqlite3").path
            )
        )
    }

    func testZeroOutDeletesLegacyDatabaseManagedFilesAndArtworkWithoutBackup() async throws {
        let container = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: container) }

        let targetRoot = container.appendingPathComponent("Target", isDirectory: true)
        let databaseURL = targetRoot.appendingPathComponent("library.sqlite3")
        let mediaRoot = targetRoot.appendingPathComponent("Music", isDirectory: true)
        let artworkRoot = targetRoot.appendingPathComponent("Artwork", isDirectory: true)
        let targetClient = try RustPlayerClient(
            dbURL: databaseURL,
            mediaRootURL: mediaRoot,
            repoRoot: container
        )
        let managedAudio = mediaRoot
            .appendingPathComponent("Album", isDirectory: true)
            .appendingPathComponent("track.mp3")
        let cachedArtwork = artworkRoot
            .appendingPathComponent("Assets", isDirectory: true)
            .appendingPathComponent("cover.png")
        try FileManager.default.createDirectory(
            at: managedAudio.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try FileManager.default.createDirectory(
            at: cachedArtwork.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("legacy database".utf8).write(to: databaseURL)
        try Data("legacy wal".utf8).write(
            to: URL(fileURLWithPath: databaseURL.path + "-wal")
        )
        try Data("legacy shm".utf8).write(
            to: URL(fileURLWithPath: databaseURL.path + "-shm")
        )
        try Data("managed audio".utf8).write(to: managedAudio)
        try Data("cached artwork".utf8).write(to: cachedArtwork)

        let model = AppModel(client: targetClient)
        model.isPlaying = true
        model.isAudioInterrupted = true
        model.playbackElapsedMS = 42_000
        model.playbackError = "legacy playback error"

        await model.zeroOutLibrary()

        let backupsRoot = targetRoot.appendingPathComponent("Backups", isDirectory: true)
        XCTAssertEqual(model.status, "Library cleared")
        XCTAssertEqual(model.playbackDetail, "Database and managed music files deleted")
        XCTAssertFalse(model.isPlaying)
        XCTAssertFalse(model.isAudioInterrupted)
        XCTAssertEqual(model.playbackElapsedMS, 0)
        XCTAssertEqual(model.playbackError, "")
        XCTAssertTrue(try targetClient.library().isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: mediaRoot.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: artworkRoot.path))
        XCTAssertFalse(
            FileManager.default.fileExists(atPath: databaseURL.path + "-wal")
        )
        XCTAssertFalse(
            FileManager.default.fileExists(atPath: databaseURL.path + "-shm")
        )
        XCTAssertNil(model.lastLibraryBackupURL)
        XCTAssertFalse(FileManager.default.fileExists(atPath: backupsRoot.path))
    }
}

@MainActor
final class AppModelAudioInterruptionTests: XCTestCase {
    func testPausedInterruptionDoesNotActivateAudioSessionWhenSystemAllowsResume() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        let model = AppModel(client: client)
        let integration = RecordingPlaybackSystemIntegration()
        model.installPlaybackSystemIntegration(integration)

        await model.handleAudioInterruptionBegan()
        await model.play(TrackItem(
            id: "blocked",
            title: "Blocked",
            artist: "Artist",
            durationMS: 1_000,
            path: root.appendingPathComponent("blocked.ogg").path
        ))
        XCTAssertEqual(model.status, "Wait for the audio interruption to end")
        await model.handleAudioInterruptionEnded(systemShouldResume: true)

        XCTAssertEqual(integration.prepareCount, 0)
        XCTAssertFalse(model.isPlaying)
        XCTAssertFalse(model.isAudioInterrupted)
    }
}

@MainActor
private final class RecordingPlaybackSystemIntegration: PlaybackSystemIntegration {
    private(set) var prepareCount = 0
    private(set) var backgroundCount = 0

    func start() {}

    func prepareForPlayback() throws {
        prepareCount += 1
    }

    func applicationDidEnterBackground() throws {
        backgroundCount += 1
    }

    func playbackPositionDidChange() {}

    func playbackDidStop() {}

    func shutdown() {}
}
