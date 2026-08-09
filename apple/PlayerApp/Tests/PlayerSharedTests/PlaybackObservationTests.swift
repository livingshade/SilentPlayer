import Combine
import Foundation
import XCTest
@testable import PlayerShared


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

        model.playback.elapsedMS = 500
        model.playback.elapsedMS = 1_000
        model.playback.isPlaying = false
        model.playback.nowPlaying = nil

        XCTAssertEqual(observedStates.count, 1)

        model.playback.nowPlaying = playerSharedTestsBuildAnchor
        XCTAssertEqual(observedStates.count, 2)

        model.playback.isPlaying = true
        XCTAssertEqual(observedStates.count, 3)

        withExtendedLifetime(observation) {}
    }
}
