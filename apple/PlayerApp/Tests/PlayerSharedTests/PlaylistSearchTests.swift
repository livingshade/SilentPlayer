import Foundation
import XCTest
@testable import PlayerShared

final class PlaylistSearchTests: XCTestCase {
    func testSearchReturnsMatchingPlaylistItems() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music"),
            repoRoot: repositoryRoot
        )
        try client.createPlaylist(name: "夜间 Jazz")
        try client.createPlaylist(name: "Road Trip")
        let all = try client.playlists()
        let jazz = try XCTUnwrap(all.first { $0.name == "夜间 Jazz" })

        XCTAssertEqual(try client.searchPlaylists(" 夜间 jAZ "), [jazz])
        XCTAssertEqual(try client.searchPlaylists("missing"), [])
        XCTAssertEqual(try client.searchPlaylists(""), all)
    }
}
