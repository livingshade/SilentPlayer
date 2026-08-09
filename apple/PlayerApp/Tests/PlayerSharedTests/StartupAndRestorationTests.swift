import Foundation
import XCTest
@testable import PlayerShared


@MainActor
final class AppModelStartupTests: XCTestCase {
    func testStartupFailureBecomesVisibleStateInsteadOfCrashing() async {
        let model = AppModel(discoverClient: {
            throw RustPlayerError.startupFailed("test startup failure")
        })

        XCTAssertEqual(model.operations.status, "Player unavailable")
        XCTAssertEqual(
            model.startupError,
            "Unable to start the player service: test startup failure"
        )
        XCTAssertEqual(model.playback.error, model.startupError)

        await model.bootstrap()

        XCTAssertEqual(model.operations.status, "Player unavailable")
        XCTAssertEqual(model.playback.error, model.startupError)
        XCTAssertTrue(model.library.tracks.isEmpty)
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

        XCTAssertEqual(model.library.scope, .playlist("Road Trip"))
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

        XCTAssertEqual(model.library.scope, .library)
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

        model.library.query = "missing"
        await model.search()

        XCTAssertEqual(model.library.scope, .playlist("Road Trip"))
        XCTAssertEqual(model.operations.status, "No songs found in Road Trip")
    }
}
