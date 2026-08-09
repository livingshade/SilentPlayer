import Foundation
import XCTest
@testable import PlayerShared


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
        XCTAssertEqual(model.operations.status, "Wait for the audio interruption to end")
        await model.handleAudioInterruptionEnded(systemShouldResume: true)

        XCTAssertEqual(integration.prepareCount, 0)
        XCTAssertFalse(model.playback.isPlaying)
        XCTAssertFalse(model.playback.isAudioInterrupted)
    }
}

@MainActor
final class RecordingPlaybackSystemIntegration: PlaybackSystemIntegration {
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
