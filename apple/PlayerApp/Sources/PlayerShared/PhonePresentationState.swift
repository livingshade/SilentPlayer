import Foundation

public enum PhonePresentationTab: String, CaseIterable, Codable, Hashable, Sendable {
    case library
    case playlists
}

public enum PhonePresentationScopeKind: String, Codable, Hashable, Sendable {
    case library
    case history
    case playlist
}

public struct PhonePresentationScope: Codable, Equatable, Sendable {
    public let kind: PhonePresentationScopeKind
    public let playlistID: Int64?

    public init(kind: PhonePresentationScopeKind, playlistID: Int64? = nil) {
        self.kind = kind
        self.playlistID = playlistID
    }

    public static let library = PhonePresentationScope(kind: .library)
    public static let history = PhonePresentationScope(kind: .history)

    public static func playlist(_ id: Int64) -> PhonePresentationScope {
        PhonePresentationScope(kind: .playlist, playlistID: id)
    }

    public var restorationScope: RestorableLibraryScope {
        switch kind {
        case .library:
            return .library
        case .history:
            return .history
        case .playlist:
            guard let playlistID else {
                return .library
            }
            return .playlist(playlistID)
        }
    }
}

public struct PhonePresentationSnapshot: Codable, Equatable, Sendable {
    public let selectedTab: PhonePresentationTab
    public let contentScope: PhonePresentationScope
    public let playlistDetailID: Int64?
    public let selectedTrackID: String?

    public init(
        selectedTab: PhonePresentationTab,
        contentScope: PhonePresentationScope,
        playlistDetailID: Int64?,
        selectedTrackID: String?
    ) {
        self.selectedTab = selectedTab
        self.contentScope = contentScope
        self.playlistDetailID = playlistDetailID
        self.selectedTrackID = selectedTrackID
    }

    public static let initial = PhonePresentationSnapshot(
        selectedTab: .library,
        contentScope: .library,
        playlistDetailID: nil,
        selectedTrackID: nil
    )

    public var bootstrapScope: RestorableLibraryScope {
        if selectedTab == .playlists, let playlistDetailID {
            return .playlist(playlistDetailID)
        }
        return contentScope.restorationScope
    }

    public func validated(against playlists: [PlaylistItem]) -> PhonePresentationSnapshot {
        let playlistIDs = Set(playlists.map(\.id))
        let validatedScope: PhonePresentationScope
        switch contentScope.kind {
        case .playlist:
            if let playlistID = contentScope.playlistID, playlistIDs.contains(playlistID) {
                validatedScope = contentScope
            } else {
                validatedScope = .library
            }
        case .library, .history:
            validatedScope = contentScope
        }

        let validatedDetailID = playlistDetailID.flatMap { id in
            playlistIDs.contains(id) ? id : nil
        }

        return PhonePresentationSnapshot(
            selectedTab: selectedTab,
            contentScope: validatedScope,
            playlistDetailID: validatedDetailID,
            selectedTrackID: selectedTrackID
        )
    }
}

public enum PhonePresentationPersistence {
    public static func encode(_ snapshot: PhonePresentationSnapshot) -> String? {
        guard let data = try? JSONEncoder().encode(snapshot) else {
            return nil
        }
        return data.base64EncodedString()
    }

    public static func decode(_ encoded: String?) -> PhonePresentationSnapshot? {
        guard let encoded,
              let data = Data(base64Encoded: encoded),
              let snapshot = try? JSONDecoder().decode(PhonePresentationSnapshot.self, from: data)
        else {
            return nil
        }
        return snapshot
    }
}
