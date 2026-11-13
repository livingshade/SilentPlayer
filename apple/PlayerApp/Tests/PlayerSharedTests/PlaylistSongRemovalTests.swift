import Foundation
import XCTest
@testable import PlayerShared

@MainActor
final class PlaylistSongRemovalTests: XCTestCase {
    func testRemovingPlaylistSongsKeepsLibraryAndDeletingThemRefreshesFilteredScope() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music"),
            repoRoot: repositoryRoot
        )
        _ = try client.importFolder(repositoryRoot.appendingPathComponent("test-assets/audio"))
        let tracks = try client.library()
        XCTAssertEqual(tracks.count, 3)
        for track in tracks.prefix(2) {
            try client.addToPlaylist(name: "Road", path: track.path)
            try client.addToPlaylist(name: "Other", path: track.path)
        }
        let model = AppModel(client: client)
        await model.refreshPlaylists()
        let playlist = try XCTUnwrap(model.playlists.items.first { $0.name == "Road" })
        await model.showPlaylist(playlist)
        await model.clearActivePlaylist()
        XCTAssertTrue(try client.playlistTracks(name: "Road").isEmpty)
        XCTAssertEqual(try client.library().count, 3)
        for track in tracks.prefix(2) {
            XCTAssertTrue(FileManager.default.fileExists(atPath: track.path))
            try client.addToPlaylist(name: "Road", path: track.path)
        }
        model.library.query = "no matching songs"
        await model.search()
        XCTAssertTrue(model.library.tracks.isEmpty)
        model.library.selectedTrack = tracks[0]

        await model.deletePlaylistSongsFromLibrary(named: "Road")

        XCTAssertEqual(try client.library().map(\.path), [tracks[2].path])
        XCTAssertTrue(try client.playlistTracks(name: "Other").isEmpty)
        XCTAssertEqual(model.activePlaylistName, "Road")
        XCTAssertEqual(model.playlists.items.first { $0.name == "Road" }?.trackCount, 0)
        XCTAssertNil(model.library.selectedTrack)
        XCTAssertFalse(model.operations.isBusy)
        for track in tracks.prefix(2) {
            XCTAssertFalse(FileManager.default.fileExists(atPath: track.path))
        }
    }
}
