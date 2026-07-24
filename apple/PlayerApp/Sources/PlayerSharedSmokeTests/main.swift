import Darwin
import Foundation
import PlayerShared

nonisolated(unsafe) private var failures: [String] = []
private let resultFilePath = CommandLine.arguments.dropFirst().first

private func expect(_ condition: @autoclosure () -> Bool, _ message: String) {
    if !condition() {
        failures.append(message)
    }
}

private func testTrackItemUsesDisplayDefaultsForBlankMetadata() {
    let track = TrackItem(
        id: "audio:blank",
        title: "  ",
        artist: "\n",
        album: "",
        durationMS: nil,
        path: "/tmp/blank.wav"
    )

    expect(track.title == "Untitled", "blank title should display as Untitled")
    expect(track.artist == "Unknown Artist", "blank artist should display as Unknown Artist")
    expect(track.album == "Unknown Album", "blank album should display as Unknown Album")
    expect(track.subtitle == "Unknown Artist - Unknown Album", "subtitle should use display defaults")
}

private func testTrackItemTrimsDisplayMetadata() {
    let track = TrackItem(
        id: "audio:trimmed",
        title: "  Song Name  ",
        artist: "  Artist Name  ",
        album: "  Album Name  ",
        durationMS: 61_000,
        path: "/tmp/song.wav"
    )

    expect(track.title == "Song Name", "title should be trimmed")
    expect(track.artist == "Artist Name", "artist should be trimmed")
    expect(track.album == "Album Name", "album should be trimmed")
    expect(track.subtitle == "Artist Name - Album Name", "subtitle should use trimmed metadata")
    expect(track.durationText == "1:01", "duration should format milliseconds as m:ss")
}

private func testTrackItemKeepsViewIdentityDefaults() {
    let track = TrackItem(
        id: "audio:hash",
        title: "Song",
        artist: "Artist",
        durationMS: nil,
        path: "/tmp/song.wav"
    )

    expect(track.viewID == "audio:hash", "viewID should default to track id")
    expect(track.primaryViewID == "audio:hash", "primaryViewID should default to track id")
    expect(track.isPrimaryView, "new TrackItem should default to primary view")
    expect(track.viewKind == "primary", "view kind should default to primary")
}

private func testTrackItemKeepsOptionalViewName() {
    let named = TrackItem(
        id: "audio:named",
        viewName: "  Evening edit  ",
        title: "Song",
        artist: "Artist",
        durationMS: nil,
        path: "/tmp/song.wav"
    )
    let unnamed = TrackItem(
        id: "audio:unnamed",
        viewName: "  ",
        title: "Song",
        artist: "Artist",
        durationMS: nil,
        path: "/tmp/song.wav"
    )

    expect(named.viewName == "Evening edit", "view name should be trimmed")
    expect(unnamed.viewName == nil, "blank view name should be nil")
}

private func testPreferredDefaultViewUsesRustArtworkPriority() {
    let primary = TrackItem(
        id: "audio:hash",
        title: "Song",
        artist: "Artist",
        durationMS: nil,
        defaultViewPriority: 1,
        path: "/tmp/primary.wav"
    )
    let coveredView = TrackItem(
        id: "audio:hash:view:artwork",
        primaryViewID: "audio:hash",
        isPrimaryView: false,
        viewKind: "derived",
        title: "Song",
        artist: "Artist",
        durationMS: nil,
        artworkURL: URL(fileURLWithPath: "/tmp/cover.jpg"),
        defaultViewPriority: 4,
        path: "/tmp/covered.wav"
    )

    let preferred = TrackItem.preferredDefaultView(in: [primary, coveredView])
    expect(preferred?.id == coveredView.id, "covered view should replace a bare primary by default")
}

private func testPlaceholderDetailsUseTrackDisplayMetadata() {
    let track = TrackItem(
        id: "audio:hash",
        viewID: "view:one",
        primaryViewID: "view:primary",
        isPrimaryView: false,
        viewKind: "derived",
        title: "",
        artist: "",
        album: "",
        durationMS: nil,
        path: "/tmp/song.wav",
        qualityProfile: "original",
        formatName: "wav"
    )
    let details = TrackDetails.placeholder(for: track)

    expect(details.viewID == "view:one", "placeholder should keep view id")
    expect(details.primaryViewID == "view:primary", "placeholder should keep primary view id")
    expect(details.displayTitle == "Untitled", "placeholder title should use display default")
    expect(details.displayArtist == "Unknown Artist", "placeholder artist should use display default")
    expect(details.displayAlbum == "Unknown Album", "placeholder album should use display default")
    expect(details.formatName == "wav", "placeholder should keep known format")
}

testTrackItemUsesDisplayDefaultsForBlankMetadata()
testTrackItemTrimsDisplayMetadata()
testTrackItemKeepsViewIdentityDefaults()
testTrackItemKeepsOptionalViewName()
testPreferredDefaultViewUsesRustArtworkPriority()
testPlaceholderDetailsUseTrackDisplayMetadata()

if failures.isEmpty {
    if let resultFilePath {
        let resultURL = URL(fileURLWithPath: resultFilePath)
        try? FileManager.default.createDirectory(
            at: resultURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try? "passed\n".write(to: resultURL, atomically: true, encoding: .utf8)
    }
    print("PlayerSharedSmokeTests passed (6 cases)")
} else {
    for failure in failures {
        fputs("PlayerSharedSmokeTests failure: \(failure)\n", stderr)
    }
    exit(1)
}
