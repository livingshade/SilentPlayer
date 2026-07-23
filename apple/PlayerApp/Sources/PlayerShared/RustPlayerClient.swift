import Foundation
import PlayerRustFFI

public struct ImportSummary: Hashable, Sendable {
    public let imported: Int
    public let copied: Int
    public let duplicatesSkipped: Int
    public let artworkCached: Int
    public let metadataWarnings: Int
}

public struct LibraryPackageSummary: Hashable, Sendable {
    public let tracks: Int
    public let audioFiles: Int
    public let sidecarFiles: Int
}

public struct AnalysisSummary: Hashable, Sendable {
    public let tracksAnalyzed: Int
    public let trackFailures: Int
    public let albumsAnalyzed: Int
    public let albumTracksUpdated: Int
    public let albumSkipped: Int
}

public struct AuditSummary: Hashable, Sendable {
    public let tracksScanned: Int
    public let hashesUpdated: Int
    public let duplicateGroups: Int
    public let tracksMerged: Int
    public let failures: Int
}

public struct UserData: Hashable, Sendable {
    public let userID: String
    public let displayName: String
    public let syncEnabled: Bool
    public let profileURL: URL
    public let historyURL: URL
    public let createdAtUnixSeconds: Int64
}

public struct PlaylistItem: Identifiable, Hashable, Sendable {
    public let id: Int64
    public let name: String
    public let trackCount: Int
    public let artworkURL: URL?
    public let artworkSource: String?
}

public struct PlaybackSnapshot: Hashable, Sendable {
    public let isPlaying: Bool
    public let positionMS: Int
    public let currentTrack: TrackItem?
    public let queueLen: Int
    public let queuePosition: Int?
    public let repeatMode: PlaybackRepeatMode
    public let shuffleEnabled: Bool
    public let gainDB: Double?
    public let loudnessStatus: String?
    public let error: String?
    public let interruptionActive: Bool
    public let resumeAfterInterruption: Bool
}

public struct PlaybackQueue: Hashable, Sendable {
    public let tracks: [TrackItem]
    public let currentIndex: Int?
    public let repeatMode: PlaybackRepeatMode
    public let shuffleEnabled: Bool
}

public struct TrackDetails: Hashable, Sendable {
    public let viewID: String
    public let primaryViewID: String
    public let isPrimaryView: Bool
    public let viewKind: String
    public let viewName: String?
    public let rating: Int?
    public let transformSpec: String?
    public let qualityProfile: String?
    public let formatName: String?
    public let artworkURL: URL?
    public let artworkSource: String?
    public let lyricsURL: URL?
    public let lyricsText: String?
    public let notes: String?
    public let audioHash: String
    public let originalTitle: String
    public let originalArtist: String
    public let originalAlbum: String
    public let displayTitle: String
    public let displayArtist: String
    public let displayAlbum: String

    public static func placeholder(for track: TrackItem) -> TrackDetails {
        TrackDetails(
            viewID: track.viewID,
            primaryViewID: track.primaryViewID,
            isPrimaryView: track.isPrimaryView,
            viewKind: track.viewKind,
            viewName: track.viewName,
            rating: track.rating,
            transformSpec: nil,
            qualityProfile: track.qualityProfile,
            formatName: track.formatName,
            artworkURL: nil,
            artworkSource: nil,
            lyricsURL: nil,
            lyricsText: nil,
            notes: nil,
            audioHash: track.id,
            originalTitle: track.title,
            originalArtist: track.artist,
            originalAlbum: track.album,
            displayTitle: track.title,
            displayArtist: track.artist,
            displayAlbum: track.album
        )
    }

    public var hasLyrics: Bool {
        guard let lyricsText else {
            return false
        }
        return !lyricsText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    public var diagnostics: [TrackViewDiagnostic] {
        var items: [TrackViewDiagnostic] = []
        if viewID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            items.append(.init(
                severity: .error,
                title: "Missing view id",
                detail: "Playback may still use the file path, but view identity and export lineage are incomplete."
            ))
        }
        if primaryViewID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            items.append(.init(
                severity: .error,
                title: "Missing primary view",
                detail: "This view cannot be traced back to an imported primary view."
            ))
        }
        if audioHash.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            items.append(.init(
                severity: .warning,
                title: "Missing audio hash",
                detail: "Playback can continue from the file path, but deduplication and primary identity are limited."
            ))
        }
        if formatName?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true {
            items.append(.init(
                severity: .warning,
                title: "Unknown format",
                detail: "Playback can still try the current file. Export uses the current audio bytes."
            ))
        }
        if qualityProfile?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true {
            items.append(.init(
                severity: .info,
                title: "Quality profile not set",
                detail: "This is expected for imported primary views until transcode profiles are implemented."
            ))
        }
        if !isPrimaryView && (transformSpec?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true) {
            items.append(.init(
                severity: .warning,
                title: "Missing transform spec",
                detail: "The view can play, but the derived-view recipe has not been recorded."
            ))
        }
        if artworkURL == nil {
            items.append(.init(
                severity: .info,
                title: "No artwork",
                detail: "A placeholder cover is shown. Playback is unaffected."
            ))
        }
        if !hasLyrics {
            items.append(.init(
                severity: .info,
                title: "No lyrics",
                detail: "Lyrics are optional and do not affect playback."
            ))
        }
        return items
    }
}

public struct AlbumArtworkSummary: Hashable, Sendable {
    public let tracksUpdated: Int
}

public struct TrackViewEdit: Encodable, Hashable, Sendable {
    public let viewName: String
    public let title: String
    public let artist: String
    public let album: String
    public let notes: String
    public let artworkPath: String?
    public let lyricsPath: String?

    public init(
        viewName: String,
        title: String,
        artist: String,
        album: String,
        notes: String,
        artworkPath: String?,
        lyricsPath: String?
    ) {
        self.viewName = viewName
        self.title = title
        self.artist = artist
        self.album = album
        self.notes = notes
        self.artworkPath = artworkPath
        self.lyricsPath = lyricsPath
    }

    enum CodingKeys: String, CodingKey {
        case viewName = "view_name"
        case title
        case artist
        case album
        case notes
        case artworkPath = "artwork_path"
        case lyricsPath = "lyrics_path"
    }
}

public enum TrackViewDiagnosticSeverity: String, Hashable, Sendable {
    case error
    case warning
    case info
}

public struct TrackViewDiagnostic: Identifiable, Hashable, Sendable {
    public let id: String
    public let severity: TrackViewDiagnosticSeverity
    public let title: String
    public let detail: String

    public init(severity: TrackViewDiagnosticSeverity, title: String, detail: String) {
        self.id = "\(severity.rawValue):\(title):\(detail)"
        self.severity = severity
        self.title = title
        self.detail = detail
    }
}

public enum RustPlayerError: LocalizedError, Sendable {
    case startupFailed(String)
    case callFailed(String)

    public var errorDescription: String? {
        switch self {
        case .startupFailed(let message), .callFailed(let message):
            return message
        }
    }
}

public final class RustPlayerClient: @unchecked Sendable {
    public let dbURL: URL
    public let repoRoot: URL
    public let mediaRootURL: URL

    private let app: OpaquePointer
    private let queue = DispatchQueue(label: "normalplayer.rust-ffi")
    private let decoder: JSONDecoder

    public static func discover() throws -> RustPlayerClient {
        let env = ProcessInfo.processInfo.environment
        let repoRoot = URL(fileURLWithPath: env["PLAYER_REPO_ROOT"] ?? Self.defaultRepoRootPath())
        let libraryRoot = Self.defaultLibraryRootURL()
        let dbURL = URL(fileURLWithPath: env["PLAYER_DB"] ?? libraryRoot
            .appendingPathComponent("player_library.sqlite3")
            .path)
        let mediaRootURL = URL(fileURLWithPath: env["PLAYER_MEDIA_ROOT"] ?? libraryRoot
            .appendingPathComponent("Music", isDirectory: true)
            .path)
        return try RustPlayerClient(dbURL: dbURL, mediaRootURL: mediaRootURL, repoRoot: repoRoot)
    }

    public init(dbURL: URL, mediaRootURL: URL? = nil, repoRoot: URL) throws {
        self.dbURL = dbURL
        self.mediaRootURL = mediaRootURL ?? dbURL.deletingLastPathComponent().appendingPathComponent("Music", isDirectory: true)
        self.repoRoot = repoRoot
        self.decoder = JSONDecoder()
        self.decoder.keyDecodingStrategy = .convertFromSnakeCase

        try FileManager.default.createDirectory(
            at: dbURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try FileManager.default.createDirectory(
            at: self.mediaRootURL,
            withIntermediateDirectories: true
        )

        let mediaRootPath = self.mediaRootURL.path
        guard let app = dbURL.path.withCString({ dbPath in
            mediaRootPath.withCString { mediaRootPath in
                player_app_create(dbPath, mediaRootPath)
            }
        }) else {
            throw RustPlayerError.startupFailed("Rust player service failed to start")
        }
        self.app = app
    }

    deinit {
        queue.sync {
            player_app_destroy(app)
        }
    }

    public func exportLibrary(to packageURL: URL) throws -> LibraryPackageSummary {
        try sync {
            try packageURL.path.withCString { packagePath in
                try decode(
                    player_app_export_library(app, packagePath),
                    as: LibraryPackageSummaryDTO.self
                ).model
            }
        }
    }

    public func importLibrary(from packageURL: URL) throws -> LibraryPackageSummary {
        try sync {
            try packageURL.path.withCString { packagePath in
                try decode(
                    player_app_import_library(app, packagePath),
                    as: LibraryPackageSummaryDTO.self
                ).model
            }
        }
    }

    public func zeroOutLibrary() throws {
        try sync {
            _ = try decode(player_app_zero_out_library(app), as: EmptyDTO.self)
        }
    }

    public func importFolder(_ folder: URL) throws -> ImportSummary {
        try sync {
            try folder.path.withCString { folderPath in
                try decode(player_app_import_folder(app, folderPath), as: ImportSummaryDTO.self).model
            }
        }
    }

    public func importFiles(_ files: [URL]) throws -> ImportSummary {
        try sync {
            let paths = files.map(\.path)
            let payload = try JSONEncoder().encode(paths)
            guard let pathsJSON = String(data: payload, encoding: .utf8) else {
                throw RustPlayerError.callFailed("Unable to encode import file list")
            }
            return try pathsJSON.withCString { value in
                try decode(player_app_import_files(app, value), as: ImportSummaryDTO.self).model
            }
        }
    }

    public func library() throws -> [TrackItem] {
        try sync {
            try decode(player_app_library(app), as: [TrackDTO].self).map(\.model)
        }
    }

    public func search(_ query: String, limit: Int = 200) throws -> [TrackItem] {
        try sync {
            try query.withCString { queryValue in
                try decode(player_app_search(app, queryValue, limit), as: [TrackDTO].self).map(\.model)
            }
        }
    }

    public func analyze() throws -> AnalysisSummary {
        try sync {
            try decode(player_app_analyze(app), as: AnalysisSummaryDTO.self).model
        }
    }

    public func auditDatabase() throws -> AuditSummary {
        try sync {
            try decode(player_app_audit_database(app), as: AuditSummaryDTO.self).model
        }
    }

    public func userData() throws -> UserData {
        try sync {
            try decode(player_app_user_data(app), as: UserDataDTO.self).model
        }
    }

    public func play(path: String) throws -> PlaybackSnapshot {
        try sync {
            try path.withCString { pathValue in
                try decode(player_app_play_path(app, pathValue), as: PlaybackSnapshotDTO.self).model
            }
        }
    }

    public func playQueue(paths: [String], startPath: String) throws -> PlaybackSnapshot {
        let payload = try JSONEncoder().encode(paths)
        let json = String(decoding: payload, as: UTF8.self)
        return try sync {
            try json.withCString { pathsValue in
                try startPath.withCString { startPathValue in
                    try decode(player_app_play_queue(app, pathsValue, startPathValue), as: PlaybackSnapshotDTO.self).model
                }
            }
        }
    }

    public func pause() throws -> PlaybackSnapshot {
        try sync {
            try decode(player_app_pause(app), as: PlaybackSnapshotDTO.self).model
        }
    }

    public func resume() throws -> PlaybackSnapshot {
        try sync {
            try decode(player_app_resume(app), as: PlaybackSnapshotDTO.self).model
        }
    }

    public func audioInterruptionBegan() throws -> PlaybackSnapshot {
        try sync {
            try decode(player_app_audio_interruption_began(app), as: PlaybackSnapshotDTO.self).model
        }
    }

    public func audioInterruptionEnded(systemShouldResume: Bool) throws -> PlaybackSnapshot {
        try sync {
            try decode(
                player_app_audio_interruption_ended(app, systemShouldResume),
                as: PlaybackSnapshotDTO.self
            ).model
        }
    }

    public func audioOutputDisconnected() throws -> PlaybackSnapshot {
        try sync {
            try decode(player_app_audio_output_disconnected(app), as: PlaybackSnapshotDTO.self).model
        }
    }

    public func stop() throws -> PlaybackSnapshot {
        try sync {
            try decode(player_app_stop(app), as: PlaybackSnapshotDTO.self).model
        }
    }

    public func next() throws -> PlaybackSnapshot {
        try sync {
            try decode(player_app_next(app), as: PlaybackSnapshotDTO.self).model
        }
    }

    public func previous() throws -> PlaybackSnapshot {
        try sync {
            try decode(player_app_previous(app), as: PlaybackSnapshotDTO.self).model
        }
    }

    public func seek(positionMS: Int) throws -> PlaybackSnapshot {
        try sync {
            try decode(player_app_seek(app, UInt64(max(0, positionMS))), as: PlaybackSnapshotDTO.self).model
        }
    }

    public func poll() throws -> PlaybackSnapshot {
        try sync {
            try decode(player_app_poll(app), as: PlaybackSnapshotDTO.self).model
        }
    }

    public func setRepeatMode(_ repeatMode: PlaybackRepeatMode) throws -> PlaybackSnapshot {
        try sync {
            try repeatMode.apiValue.withCString { repeatModeValue in
                try decode(player_app_set_repeat_mode(app, repeatModeValue), as: PlaybackSnapshotDTO.self).model
            }
        }
    }

    public func setShuffle(enabled: Bool) throws -> PlaybackSnapshot {
        try sync {
            try decode(player_app_set_shuffle(app, enabled), as: PlaybackSnapshotDTO.self).model
        }
    }

    public func queueSnapshot() throws -> PlaybackQueue {
        try sync {
            try decode(player_app_queue(app), as: PlaybackQueueDTO.self).model
        }
    }

    public func trackDetails(path: String) throws -> TrackDetails {
        try sync {
            try path.withCString { pathValue in
                try decode(player_app_track_details(app, pathValue), as: TrackDetailsDTO.self).model
            }
        }
    }

    public func editTrackView(path: String, edit: TrackViewEdit) throws -> TrackItem {
        let payload = try JSONEncoder().encode(edit)
        let json = String(decoding: payload, as: UTF8.self)
        return try sync {
            try path.withCString { pathValue in
                try json.withCString { editValue in
                    try decode(player_app_edit_track_view(app, pathValue, editValue), as: TrackDTO.self).model
                }
            }
        }
    }

    public func setTrackNotes(path: String, notes: String) throws -> TrackItem {
        try sync {
            try path.withCString { pathValue in
                try notes.withCString { notesValue in
                    try decode(player_app_set_track_notes(app, pathValue, notesValue), as: TrackDTO.self).model
                }
            }
        }
    }

    public func setTrackRating(path: String, rating: Int?) throws -> TrackItem {
        let ratingValue = rating ?? 0
        return try sync {
            try path.withCString { pathValue in
                try decode(player_app_set_track_rating(app, pathValue, Int32(ratingValue)), as: TrackDTO.self).model
            }
        }
    }

    public func setTrackMetadata(path: String, title: String, artist: String, album: String) throws -> TrackItem {
        try sync {
            try path.withCString { pathValue in
                try title.withCString { titleValue in
                    try artist.withCString { artistValue in
                        try album.withCString { albumValue in
                            try decode(
                                player_app_set_track_metadata(app, pathValue, titleValue, artistValue, albumValue),
                                as: TrackDTO.self
                            ).model
                        }
                    }
                }
            }
        }
    }

    public func setTrackArtwork(path: String, imageURL: URL) throws -> TrackItem {
        try sync {
            try path.withCString { pathValue in
                try imageURL.path.withCString { imagePath in
                    try decode(player_app_set_track_artwork(app, pathValue, imagePath), as: TrackDTO.self).model
                }
            }
        }
    }

    public func setAlbumArtwork(path: String, imageURL: URL) throws -> AlbumArtworkSummary {
        try sync {
            try path.withCString { pathValue in
                try imageURL.path.withCString { imagePath in
                    try decode(
                        player_app_set_album_artwork(app, pathValue, imagePath),
                        as: AlbumArtworkSummaryDTO.self
                    ).model
                }
            }
        }
    }

    public func setTrackLyrics(path: String, lyricsURL: URL) throws -> TrackItem {
        try sync {
            try path.withCString { pathValue in
                try lyricsURL.path.withCString { lyricsPath in
                    try decode(player_app_set_track_lyrics(app, pathValue, lyricsPath), as: TrackDTO.self).model
                }
            }
        }
    }

    public func exportTrackView(path: String, destinationURL: URL) throws -> TrackItem {
        try sync {
            try path.withCString { pathValue in
                try destinationURL.path.withCString { destinationPath in
                    try decode(player_app_export_track_view(app, pathValue, destinationPath), as: TrackDTO.self).model
                }
            }
        }
    }

    public func setFavorite(path: String, enabled: Bool) throws {
        try sync {
            try path.withCString { pathValue in
                _ = try decode(player_app_set_favorite(app, pathValue, enabled), as: EmptyDTO.self)
            }
        }
    }

    public func favorites() throws -> [TrackItem] {
        try sync {
            try decode(player_app_favorites(app), as: [TrackDTO].self).map(\.model)
        }
    }

    public func history(limit: Int = 100) throws -> [TrackItem] {
        try sync {
            try decode(player_app_history(app, limit), as: [TrackDTO].self).map(\.model)
        }
    }

    public func playlists() throws -> [PlaylistItem] {
        try sync {
            try decode(player_app_playlists(app), as: [PlaylistDTO].self).map(\.model)
        }
    }

    public func createPlaylist(name: String) throws {
        try sync {
            try name.withCString { nameValue in
                _ = try decode(player_app_create_playlist(app, nameValue), as: EmptyDTO.self)
            }
        }
    }

    public func renamePlaylist(oldName: String, newName: String) throws {
        try sync {
            try oldName.withCString { oldNameValue in
                try newName.withCString { newNameValue in
                    _ = try decode(player_app_rename_playlist(app, oldNameValue, newNameValue), as: EmptyDTO.self)
                }
            }
        }
    }

    public func setPlaylistArtwork(name: String, imageURL: URL) throws {
        try sync {
            try name.withCString { nameValue in
                try imageURL.path.withCString { imagePath in
                    _ = try decode(player_app_set_playlist_artwork(app, nameValue, imagePath), as: EmptyDTO.self)
                }
            }
        }
    }

    public func deletePlaylist(name: String) throws {
        try sync {
            try name.withCString { nameValue in
                _ = try decode(player_app_delete_playlist(app, nameValue), as: EmptyDTO.self)
            }
        }
    }

    public func clearPlaylist(name: String) throws {
        try sync {
            try name.withCString { nameValue in
                _ = try decode(player_app_clear_playlist(app, nameValue), as: EmptyDTO.self)
            }
        }
    }

    public func addToPlaylist(name: String, path: String) throws {
        try sync {
            try name.withCString { nameValue in
                try path.withCString { pathValue in
                    _ = try decode(player_app_add_to_playlist(app, nameValue, pathValue), as: EmptyDTO.self)
                }
            }
        }
    }

    public func removeFromPlaylist(name: String, path: String) throws {
        try sync {
            try name.withCString { nameValue in
                try path.withCString { pathValue in
                    _ = try decode(player_app_remove_from_playlist(app, nameValue, pathValue), as: EmptyDTO.self)
                }
            }
        }
    }

    public func movePlaylistTrack(name: String, path: String, delta: Int) throws {
        try sync {
            try name.withCString { nameValue in
                try path.withCString { pathValue in
                    _ = try decode(player_app_move_playlist_track(app, nameValue, pathValue, Int32(delta)), as: EmptyDTO.self)
                }
            }
        }
    }

    public func sortPlaylist(name: String, sort: String) throws {
        try sync {
            try name.withCString { nameValue in
                try sort.withCString { sortValue in
                    _ = try decode(player_app_sort_playlist(app, nameValue, sortValue), as: EmptyDTO.self)
                }
            }
        }
    }

    public func playlistTracks(name: String) throws -> [TrackItem] {
        try sync {
            try name.withCString { nameValue in
                try decode(player_app_playlist_tracks(app, nameValue), as: [TrackDTO].self).map(\.model)
            }
        }
    }

    private func sync<T>(_ operation: () throws -> T) throws -> T {
        try queue.sync(execute: operation)
    }

    private func decode<T: Decodable>(_ pointer: UnsafeMutablePointer<CChar>?, as type: T.Type) throws -> T {
        guard let pointer else {
            throw RustPlayerError.callFailed("Rust FFI returned a null response")
        }
        defer { player_string_free(pointer) }

        let json = String(cString: pointer)
        let response = try decoder.decode(ResponseDTO<T>.self, from: Data(json.utf8))
        guard response.ok else {
            throw RustPlayerError.callFailed(response.error ?? "Rust player call failed")
        }
        guard let data = response.data else {
            throw RustPlayerError.callFailed("Rust player returned no data")
        }
        return data
    }

    private static func defaultRepoRootPath() -> String {
        var url = URL(fileURLWithPath: #filePath)
        for _ in 0..<4 {
            url.deleteLastPathComponent()
        }
        return url.path
    }

    private static func defaultLibraryRootURL() -> URL {
        #if os(iOS)
        let baseURL = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first ?? FileManager.default.temporaryDirectory
        return baseURL.appendingPathComponent("NormalPlayer", isDirectory: true)
        #else
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Music", isDirectory: true)
            .appendingPathComponent("NormalPlayer", isDirectory: true)
        #endif
    }
}

private struct ResponseDTO<T: Decodable>: Decodable {
    let ok: Bool
    let data: T?
    let error: String?
}

private struct EmptyDTO: Decodable {}

private struct TrackDTO: Decodable {
    let id: String
    let viewId: String
    let primaryViewId: String
    let isPrimaryView: Bool
    let viewKind: String
    let viewName: String?
    let rating: Int?
    let title: String
    let artist: String?
    let album: String?
    let durationMs: UInt64?
    let artworkCount: UInt32
    let artworkPath: String?
    let artworkSource: String?
    let hasAlbumIdentity: Bool
    let path: String
    let qualityProfile: String?
    let formatName: String?
    let gainDb: Double?
    let loudnessStatus: String

    var model: TrackItem {
        TrackItem(
            id: id,
            viewID: viewId,
            primaryViewID: primaryViewId,
            isPrimaryView: isPrimaryView,
            viewKind: viewKind,
            viewName: viewName,
            rating: rating,
            title: MetadataDefaults.title(title),
            artist: MetadataDefaults.artist(artist),
            album: MetadataDefaults.album(album),
            durationMS: durationMs.map(Int.init),
            artworkCount: Int(artworkCount),
            artworkURL: artworkPath.map { URL(fileURLWithPath: $0) },
            artworkSource: artworkSource,
            hasAlbumIdentity: hasAlbumIdentity,
            path: path,
            qualityProfile: qualityProfile,
            formatName: formatName,
            gainDB: gainDb,
            loudnessStatus: loudnessStatus
        )
    }
}

private struct ImportSummaryDTO: Decodable {
    let imported: Int
    let copied: Int
    let duplicatesSkipped: Int
    let artworkCached: Int
    let metadataWarnings: Int

    var model: ImportSummary {
        ImportSummary(
            imported: imported,
            copied: copied,
            duplicatesSkipped: duplicatesSkipped,
            artworkCached: artworkCached,
            metadataWarnings: metadataWarnings
        )
    }
}

private struct LibraryPackageSummaryDTO: Decodable {
    let tracks: Int
    let audioFiles: Int
    let sidecarFiles: Int

    var model: LibraryPackageSummary {
        LibraryPackageSummary(
            tracks: tracks,
            audioFiles: audioFiles,
            sidecarFiles: sidecarFiles
        )
    }
}

private struct AnalysisSummaryDTO: Decodable {
    let tracksAnalyzed: Int
    let trackFailures: Int
    let albumsAnalyzed: Int
    let albumTracksUpdated: Int
    let albumSkipped: Int

    var model: AnalysisSummary {
        AnalysisSummary(
            tracksAnalyzed: tracksAnalyzed,
            trackFailures: trackFailures,
            albumsAnalyzed: albumsAnalyzed,
            albumTracksUpdated: albumTracksUpdated,
            albumSkipped: albumSkipped
        )
    }
}

private struct AlbumArtworkSummaryDTO: Decodable {
    let tracksUpdated: Int

    var model: AlbumArtworkSummary {
        AlbumArtworkSummary(tracksUpdated: tracksUpdated)
    }
}

private struct AuditSummaryDTO: Decodable {
    let tracksScanned: Int
    let hashesUpdated: Int
    let duplicateGroups: Int
    let tracksMerged: Int
    let failures: Int

    var model: AuditSummary {
        AuditSummary(
            tracksScanned: tracksScanned,
            hashesUpdated: hashesUpdated,
            duplicateGroups: duplicateGroups,
            tracksMerged: tracksMerged,
            failures: failures
        )
    }
}

private struct UserDataDTO: Decodable {
    let userId: String
    let displayName: String
    let syncEnabled: Bool
    let profilePath: String
    let historyPath: String
    let createdAtUnixSeconds: Int64

    var model: UserData {
        UserData(
            userID: userId,
            displayName: displayName,
            syncEnabled: syncEnabled,
            profileURL: URL(fileURLWithPath: profilePath),
            historyURL: URL(fileURLWithPath: historyPath),
            createdAtUnixSeconds: createdAtUnixSeconds
        )
    }
}

private struct PlaylistDTO: Decodable {
    let id: Int64
    let name: String
    let trackCount: Int
    let artworkPath: String?
    let artworkSource: String?

    var model: PlaylistItem {
        PlaylistItem(
            id: id,
            name: name,
            trackCount: trackCount,
            artworkURL: artworkPath.map { URL(fileURLWithPath: $0) },
            artworkSource: artworkSource
        )
    }
}

private struct PlaybackSnapshotDTO: Decodable {
    let isPlaying: Bool
    let positionMs: UInt64
    let currentTrack: TrackDTO?
    let queueLen: Int
    let queuePosition: Int?
    let repeatMode: PlaybackRepeatMode
    let shuffleEnabled: Bool
    let gainDb: Double?
    let loudnessStatus: String?
    let error: String?
    let interruptionActive: Bool
    let resumeAfterInterruption: Bool

    var model: PlaybackSnapshot {
        PlaybackSnapshot(
            isPlaying: isPlaying,
            positionMS: Int(positionMs),
            currentTrack: currentTrack?.model,
            queueLen: queueLen,
            queuePosition: queuePosition,
            repeatMode: repeatMode,
            shuffleEnabled: shuffleEnabled,
            gainDB: gainDb,
            loudnessStatus: loudnessStatus,
            error: error,
            interruptionActive: interruptionActive,
            resumeAfterInterruption: resumeAfterInterruption
        )
    }
}

private struct PlaybackQueueDTO: Decodable {
    let tracks: [TrackDTO]
    let currentIndex: Int?
    let repeatMode: PlaybackRepeatMode
    let shuffleEnabled: Bool

    var model: PlaybackQueue {
        PlaybackQueue(
            tracks: tracks.map(\.model),
            currentIndex: currentIndex,
            repeatMode: repeatMode,
            shuffleEnabled: shuffleEnabled
        )
    }
}

private struct TrackDetailsDTO: Decodable {
    let viewId: String
    let primaryViewId: String
    let isPrimaryView: Bool
    let viewKind: String
    let viewName: String?
    let rating: Int?
    let transformSpec: String?
    let qualityProfile: String?
    let formatName: String?
    let artworkPath: String?
    let artworkSource: String?
    let lyricsPath: String?
    let lyricsText: String?
    let notes: String?
    let audioHash: String
    let originalTitle: String
    let originalArtist: String?
    let originalAlbum: String?
    let displayTitle: String
    let displayArtist: String?
    let displayAlbum: String?

    var model: TrackDetails {
        TrackDetails(
            viewID: viewId,
            primaryViewID: primaryViewId,
            isPrimaryView: isPrimaryView,
            viewKind: viewKind,
            viewName: viewName,
            rating: rating,
            transformSpec: transformSpec,
            qualityProfile: qualityProfile,
            formatName: formatName,
            artworkURL: artworkPath.map { URL(fileURLWithPath: $0) },
            artworkSource: artworkSource,
            lyricsURL: lyricsPath.map { URL(fileURLWithPath: $0) },
            lyricsText: lyricsText,
            notes: notes,
            audioHash: audioHash,
            originalTitle: MetadataDefaults.title(originalTitle),
            originalArtist: MetadataDefaults.artist(originalArtist),
            originalAlbum: MetadataDefaults.album(originalAlbum),
            displayTitle: MetadataDefaults.title(displayTitle),
            displayArtist: MetadataDefaults.artist(displayArtist),
            displayAlbum: MetadataDefaults.album(displayAlbum)
        )
    }
}
