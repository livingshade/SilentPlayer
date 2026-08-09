import Foundation
import XCTest
@testable import PlayerShared


@MainActor
final class LibraryMigrationTests: XCTestCase {
    func testImportCreatesCompleteBackupBeforeReplacingLibrary() async throws {
        let container = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: container) }

        let sourceRoot = container.appendingPathComponent("Source", isDirectory: true)
        let sourceClient = try RustPlayerClient(
            dbURL: sourceRoot.appendingPathComponent("library.sqlite3"),
            mediaRootURL: sourceRoot.appendingPathComponent("Music", isDirectory: true),
            repoRoot: container
        )
        let importPackage = container
            .appendingPathComponent("Import.silentlibrary", isDirectory: true)
        _ = try sourceClient.exportLibrary(to: importPackage)

        let targetRoot = container.appendingPathComponent("Target", isDirectory: true)
        let targetClient = try RustPlayerClient(
            dbURL: targetRoot.appendingPathComponent("library.sqlite3"),
            mediaRootURL: targetRoot.appendingPathComponent("Music", isDirectory: true),
            repoRoot: container
        )
        let model = AppModel(client: targetClient)

        await model.importLibrary(from: importPackage)

        XCTAssertEqual(model.operations.status, "Library imported")
        let backupURL = try XCTUnwrap(model.operations.lastLibraryBackupURL)
        XCTAssertEqual(
            backupURL.deletingLastPathComponent(),
            targetRoot.appendingPathComponent("Backups", isDirectory: true)
        )
        XCTAssertTrue(
            FileManager.default.fileExists(
                atPath: backupURL.appendingPathComponent("manifest.json").path
            )
        )
        XCTAssertTrue(
            FileManager.default.fileExists(
                atPath: backupURL.appendingPathComponent("player_library.sqlite3").path
            )
        )
    }

    func testZeroOutDeletesLegacyDatabaseManagedFilesAndArtworkWithoutBackup() async throws {
        let container = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: container) }

        let targetRoot = container.appendingPathComponent("Target", isDirectory: true)
        let databaseURL = targetRoot.appendingPathComponent("library.sqlite3")
        let mediaRoot = targetRoot.appendingPathComponent("Music", isDirectory: true)
        let artworkRoot = targetRoot.appendingPathComponent("Artwork", isDirectory: true)
        let targetClient = try RustPlayerClient(
            dbURL: databaseURL,
            mediaRootURL: mediaRoot,
            repoRoot: container
        )
        let managedAudio = mediaRoot
            .appendingPathComponent("Album", isDirectory: true)
            .appendingPathComponent("track.mp3")
        let cachedArtwork = artworkRoot
            .appendingPathComponent("Assets", isDirectory: true)
            .appendingPathComponent("cover.png")
        try FileManager.default.createDirectory(
            at: managedAudio.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try FileManager.default.createDirectory(
            at: cachedArtwork.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("legacy database".utf8).write(to: databaseURL)
        try Data("legacy wal".utf8).write(
            to: URL(fileURLWithPath: databaseURL.path + "-wal")
        )
        try Data("legacy shm".utf8).write(
            to: URL(fileURLWithPath: databaseURL.path + "-shm")
        )
        try Data("managed audio".utf8).write(to: managedAudio)
        try Data("cached artwork".utf8).write(to: cachedArtwork)

        let model = AppModel(client: targetClient)
        model.playback.isPlaying = true
        model.playback.isAudioInterrupted = true
        model.playback.elapsedMS = 42_000
        model.playback.error = "legacy playback error"

        await model.zeroOutLibrary()

        let backupsRoot = targetRoot.appendingPathComponent("Backups", isDirectory: true)
        XCTAssertEqual(model.operations.status, "Library cleared")
        XCTAssertEqual(model.playback.detail, "Database and managed music files deleted")
        XCTAssertFalse(model.playback.isPlaying)
        XCTAssertFalse(model.playback.isAudioInterrupted)
        XCTAssertEqual(model.playback.elapsedMS, 0)
        XCTAssertEqual(model.playback.error, "")
        XCTAssertTrue(try targetClient.library().isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: mediaRoot.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: artworkRoot.path))
        XCTAssertFalse(
            FileManager.default.fileExists(atPath: databaseURL.path + "-wal")
        )
        XCTAssertFalse(
            FileManager.default.fileExists(atPath: databaseURL.path + "-shm")
        )
        XCTAssertNil(model.operations.lastLibraryBackupURL)
        XCTAssertFalse(FileManager.default.fileExists(atPath: backupsRoot.path))
    }
}
