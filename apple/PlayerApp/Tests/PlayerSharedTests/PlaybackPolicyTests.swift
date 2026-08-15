import Foundation
import XCTest
@testable import PlayerShared


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
        XCTAssertEqual(
            try JSONDecoder().decode(PlaybackMode.self, from: Data(#""repeat_one""#.utf8)),
            .repeatOne
        )
        XCTAssertEqual(PlaybackMode.sequential.apiValue, "sequential")
        XCTAssertEqual(PlaybackMode.shuffle.systemImage, "shuffle")
        XCTAssertThrowsError(
            try JSONDecoder().decode(PlaybackMode.self, from: Data(#""repeat_all""#.utf8))
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

    func testRouteChangePausesWheneverTheOldOutputBecomesUnavailable() {
        XCTAssertTrue(PlaybackRouteChangePolicy.shouldPause(
            oldDeviceBecameUnavailable: true
        ))
        XCTAssertFalse(PlaybackRouteChangePolicy.shouldPause(
            oldDeviceBecameUnavailable: false
        ))
    }

    func testRouteDisconnectUsesOnePausePathAndDoesNotCreateAStuckInterruption() {
        XCTAssertFalse(PlaybackRouteChangePolicy.prefersSystemInterruptionOnDisconnect)
        XCTAssertTrue(PlaybackRouteChangePolicy.shouldPause(
            oldDeviceBecameUnavailable: true
        ))
    }

    func testCarPowerOffStillPausesWhenPreviousRouteDescriptionIsMissing() {
        XCTAssertTrue(PlaybackRouteChangePolicy.shouldPause(
            oldDeviceBecameUnavailable: true
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

        model.playback.isPlaying = true
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
