import Foundation
import XCTest
@testable import PlayerShared

#if os(iOS)
import MediaPlayer
import UIKit
#endif

#if os(macOS)
import AppKit
import MediaPlayer
#endif

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

#if os(macOS)
@MainActor
final class MacNowPlayingArtworkFactoryTests: XCTestCase {
    func testRequestHandlerCanRunOutsideMainActor() async throws {
        let image = NSImage(size: CGSize(width: 32, height: 32))
        let artworkBox = SendableMacArtworkBox(
            artwork: MacNowPlayingArtworkFactory.make(image: image)
        )

        let returnedImage = await Task.detached {
            artworkBox.artwork.image(at: CGSize(width: 16, height: 16)) != nil
        }.value

        XCTAssertTrue(returnedImage)
    }
}

@MainActor
final class MacPlaybackSystemIntegrationTests: XCTestCase {
    func testPublishesTrackAndExplicitPlaybackStateToSystemNowPlayingCenter() throws {
        let model = AppModel(discoverClient: {
            throw RustPlayerError.startupFailed("macOS Now Playing test")
        })
        model.playback.nowPlaying = playerSharedTestsBuildAnchor
        model.playback.isPlaying = true
        model.playback.elapsedMS = 1_250
        model.playback.queueCount = 3
        model.playback.queuePosition = 1

        let integration = MacPlaybackSystemIntegration(model: model)
        integration.start()
        defer { integration.shutdown() }

        let center = MPNowPlayingInfoCenter.default()
        XCTAssertEqual(
            center.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String,
            playerSharedTestsBuildAnchor.title
        )
        XCTAssertEqual(
            center.nowPlayingInfo?[MPNowPlayingInfoPropertyElapsedPlaybackTime] as? Double,
            1.25
        )
        XCTAssertEqual(
            center.nowPlayingInfo?[MPNowPlayingInfoPropertyPlaybackQueueCount] as? Int,
            3
        )
        XCTAssertEqual(
            center.nowPlayingInfo?[MPNowPlayingInfoPropertyPlaybackQueueIndex] as? Int,
            1
        )
        XCTAssertEqual(center.playbackState, .playing)

        let commands = MPRemoteCommandCenter.shared()
        model.playback.playbackMode = .repeatOne
        integration.playbackPositionDidChange()
        XCTAssertEqual(commands.changeRepeatModeCommand.currentRepeatType, .one)
        XCTAssertEqual(commands.changeShuffleModeCommand.currentShuffleType, .off)

        model.playback.playbackMode = .shuffle
        integration.playbackPositionDidChange()
        XCTAssertEqual(commands.changeRepeatModeCommand.currentRepeatType, .off)
        XCTAssertEqual(commands.changeShuffleModeCommand.currentShuffleType, .items)

        model.playback.isPlaying = false
        integration.playbackPositionDidChange()
        XCTAssertEqual(
            center.nowPlayingInfo?[MPNowPlayingInfoPropertyPlaybackRate] as? Double,
            0
        )
        XCTAssertEqual(center.playbackState, .paused)

        integration.playbackDidStop()
        XCTAssertNil(center.nowPlayingInfo)
        XCTAssertEqual(center.playbackState, .stopped)
    }
}
#endif
