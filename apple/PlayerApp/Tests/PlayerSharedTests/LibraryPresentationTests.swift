import Combine
import Foundation
import XCTest
@testable import PlayerShared


@MainActor
final class LibraryPresentationCacheTests: XCTestCase {
    func testReturningToLibraryUsesSessionCacheUntilExplicitRefresh() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        let model = AppModel(client: client)
        var loadingMessages: [String] = []
        let observation = model.operations.$libraryStatus
            .filter { !$0.isEmpty }
            .sink { loadingMessages.append($0) }

        await model.bootstrap()
        let messagesAfterFirstLoad = loadingMessages.count
        XCTAssertGreaterThan(messagesAfterFirstLoad, 0)

        await model.bootstrap()
        XCTAssertEqual(loadingMessages.count, messagesAfterFirstLoad)

        await model.showFavorites()
        await model.showLibrary()
        XCTAssertEqual(model.library.scope, .library)
        XCTAssertEqual(loadingMessages.count, messagesAfterFirstLoad)

        await model.refreshLibrary()
        XCTAssertGreaterThan(loadingMessages.count, messagesAfterFirstLoad)
        withExtendedLifetime(observation) {}
    }
}

@MainActor
final class SinglePrimaryPresentationTests: XCTestCase {
    func testPlaylistPickerAddsExplicitTrackToChosenPlaylist() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        _ = try client.importFiles([
            repositoryRoot
                .appendingPathComponent("test-assets/audio/into_the_oceans_chorus.ogg")
        ])
        try client.createPlaylist(name: "Road")
        let playlist = try XCTUnwrap(client.playlists().first)
        let track = try XCTUnwrap(client.library().first)
        let model = AppModel(client: client)

        await model.bootstrap()
        model.presentPlaylistPicker(for: track)

        XCTAssertEqual(model.playlists.presentedSheet, .picker)
        XCTAssertEqual(model.playlists.pickerTrack?.id, track.id)

        await model.addPlaylistPickerTrack(to: playlist)

        XCTAssertNil(model.playlists.presentedSheet)
        XCTAssertNil(model.playlists.pickerTrack)
        XCTAssertEqual(try client.playlistTracks(name: playlist.name).map(\.id), [track.id])
        XCTAssertEqual(model.operations.status, "Added \(track.title) to \(playlist.name)")

        model.presentPlaylistPicker(for: track)
        await model.addPlaylistPickerTrack(to: playlist)

        XCTAssertNil(model.playlists.presentedSheet)
        XCTAssertNil(model.playlists.pickerTrack)
        XCTAssertEqual(try client.playlistTracks(name: playlist.name).map(\.id), [track.id])
        XCTAssertEqual(model.operations.status, "\(track.title) is already in \(playlist.name)")
    }

    func testCreatingPlaylistFromPickerAddsOriginalTrack() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        _ = try client.importFiles([
            repositoryRoot
                .appendingPathComponent("test-assets/audio/into_the_oceans_chorus.ogg")
        ])
        let track = try XCTUnwrap(client.library().first)
        let model = AppModel(client: client)

        await model.bootstrap()
        model.presentPlaylistPicker(for: track)
        model.presentCreatePlaylist(addingPickerTrack: true)
        model.playlists.newNameDraft = "New Road"

        XCTAssertEqual(model.playlists.presentedSheet, .create)
        XCTAssertEqual(model.playlists.pickerTrack?.id, track.id)

        await model.createPlaylist()

        XCTAssertNil(model.playlists.presentedSheet)
        XCTAssertNil(model.playlists.pickerTrack)
        XCTAssertEqual(model.library.scope, .playlist("New Road"))
        XCTAssertEqual(try client.playlistTracks(name: "New Road").map(\.id), [track.id])
        XCTAssertEqual(model.operations.status, "Created New Road and added \(track.title)")
    }

    func testFailedPlaylistAddKeepsPickerAndTargetAvailableForRetry() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        try client.createPlaylist(name: "Road")
        let playlist = try XCTUnwrap(client.playlists().first)
        let missingTrack = TrackItem(
            id: "audio:missing",
            title: "Missing",
            artist: "",
            durationMS: nil,
            path: root.appendingPathComponent("missing.flac").path
        )
        let model = AppModel(client: client)

        await model.bootstrap()
        model.presentPlaylistPicker(for: missingTrack)
        await model.addPlaylistPickerTrack(to: playlist)

        XCTAssertEqual(model.playlists.presentedSheet, .picker)
        XCTAssertEqual(model.playlists.pickerTrack?.id, missingTrack.id)
        XCTAssertEqual(model.operations.status, "Error")
        XCTAssertTrue(try client.playlistTracks(name: playlist.name).isEmpty)

        model.dismissPlaylistSheet()
        XCTAssertNil(model.playlists.presentedSheet)
        XCTAssertNil(model.playlists.pickerTrack)
    }

    func testLibraryDisplaysEveryTrackReturnedByRustWithoutClientSideCollapse() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        _ = try client.importFiles([
            repositoryRoot
                .appendingPathComponent("test-assets/audio/into_the_oceans_chorus.ogg"),
            repositoryRoot
                .appendingPathComponent("test-assets/audio/funk_room_reverb.ogg")
        ])
        let rustTracks = try client.library()
        let model = AppModel(client: client)

        await model.bootstrap()

        XCTAssertEqual(model.library.tracks.count, rustTracks.count)
        XCTAssertEqual(Set(model.library.tracks.map(\.id)), Set(rustTracks.map(\.id)))
    }

    func testMetadataEditReplacesTheSameTrackInPlace() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        _ = try client.importFiles([
            repositoryRoot
                .appendingPathComponent("test-assets/audio/into_the_oceans_chorus.ogg")
        ])
        let model = AppModel(client: client)
        await model.bootstrap()
        let original = try XCTUnwrap(model.library.tracks.first)
        model.selectTrack(id: original.id)
        model.presentTrackEdit()
        model.trackDetail.titleDraft = "Updated in place"

        await model.saveTrackEdit()

        let updated = try XCTUnwrap(model.library.selectedTrack)
        XCTAssertEqual(model.library.tracks.count, 1)
        XCTAssertEqual(updated.id, original.id)
        XCTAssertEqual(updated.path, original.path)
        XCTAssertEqual(updated.title, "Updated in place")
        XCTAssertEqual(try client.library().count, 1)
    }

    func testRemovingFromPlaylistKeepsSongWhileDeletingFromLibraryRemovesItEverywhere() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: root
        )
        _ = try client.importFiles([
            repositoryRoot
                .appendingPathComponent("test-assets/audio/into_the_oceans_chorus.ogg"),
            repositoryRoot
                .appendingPathComponent("test-assets/audio/funk_room_reverb.ogg")
        ])
        try client.createPlaylist(name: "Road")
        let playlist = try XCTUnwrap(client.playlists().first)
        let target = try XCTUnwrap(client.library().first)
        try client.addToPlaylist(name: playlist.name, path: target.path)
        let model = AppModel(client: client)
        await model.bootstrap(restoring: .playlist(playlist.id))

        await model.removeFromActivePlaylist(target)

        XCTAssertEqual(try client.library().count, 2)
        XCTAssertTrue(try client.playlistTracks(name: playlist.name).isEmpty)
        try client.addToPlaylist(name: playlist.name, path: target.path)
        XCTAssertTrue(FileManager.default.fileExists(atPath: target.path))

        await model.deleteFromLibrary(target)

        XCTAssertEqual(try client.library().count, 1)
        XCTAssertTrue(try client.playlistTracks(name: playlist.name).isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: target.path))
        XCTAssertFalse(model.library.tracks.contains(where: { $0.id == target.id }))
        XCTAssertEqual(model.operations.status, "Deleted \(target.title) from Library")
    }
}
