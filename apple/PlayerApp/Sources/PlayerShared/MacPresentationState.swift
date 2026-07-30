import Foundation

public struct MacPresentationSnapshot: Codable, Equatable, Sendable {
    public let contentScope: RestorableLibraryScope
    public let selectedTrackID: String?

    public init(
        contentScope: RestorableLibraryScope,
        selectedTrackID: String?
    ) {
        self.contentScope = contentScope
        self.selectedTrackID = selectedTrackID
    }

    public static let initial = MacPresentationSnapshot(
        contentScope: .library,
        selectedTrackID: nil
    )

    public func validated(against playlists: [PlaylistItem]) -> MacPresentationSnapshot {
        guard case .playlist(let playlistID) = contentScope else {
            return self
        }
        guard playlists.contains(where: { $0.id == playlistID }) else {
            return MacPresentationSnapshot(
                contentScope: .library,
                selectedTrackID: selectedTrackID
            )
        }
        return self
    }
}

public enum MacPresentationPersistence {
    public static func encode(_ snapshot: MacPresentationSnapshot) -> String? {
        guard let data = try? JSONEncoder().encode(snapshot) else {
            return nil
        }
        return data.base64EncodedString()
    }

    public static func decode(_ encoded: String?) -> MacPresentationSnapshot? {
        guard let encoded,
              let data = Data(base64Encoded: encoded),
              let snapshot = try? JSONDecoder().decode(MacPresentationSnapshot.self, from: data)
        else {
            return nil
        }
        return snapshot
    }
}
