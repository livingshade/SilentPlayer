import Foundation
import XCTest
@testable import PlayerShared

#if os(iOS)
import MediaPlayer
import UIKit

final class SendableArtworkBox: @unchecked Sendable {
    let artwork: MPMediaItemArtwork

    init(artwork: MPMediaItemArtwork) {
        self.artwork = artwork
    }
}
#endif

#if os(macOS)
import AppKit
import MediaPlayer

final class SendableMacArtworkBox: @unchecked Sendable {
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

let repositoryRoot = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
