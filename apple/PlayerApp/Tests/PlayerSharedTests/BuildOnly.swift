import Foundation
import XCTest
@testable import PlayerShared

let playerSharedTestsBuildAnchor = TrackItem(
    id: "audio:test-anchor",
    title: "Anchor",
    artist: "Artist",
    durationMS: nil,
    path: "/tmp/anchor.wav"
)

final class PlaybackPolicyTests: XCTestCase {
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
            isPlaying: false,
            isInterrupted: false
        ))
        XCTAssertFalse(PlaybackRemoteCommandPolicy.canPlay(
            hasTrack: true,
            isPlaying: false,
            isInterrupted: true
        ))
        XCTAssertFalse(PlaybackRemoteCommandPolicy.canTogglePlayPause(
            hasTrack: true,
            isInterrupted: true
        ))
        XCTAssertFalse(PlaybackRemoteCommandPolicy.canPlay(
            hasTrack: false,
            isPlaying: false,
            isInterrupted: false
        ))
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

#if os(macOS)
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
}
#endif

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

    func start() {}

    func prepareForPlayback() throws {
        prepareCount += 1
    }

    func playbackDidStop() {}

    func shutdown() {}
}
