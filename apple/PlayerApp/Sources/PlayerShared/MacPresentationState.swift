import Foundation

public struct MacPresentationSnapshot: Codable, Equatable, Sendable {
    public static let currentVersion = 1

    public let version: Int
    public let contentScope: RestorableLibraryScope
    public let selectedViewID: String?

    public init(
        version: Int = currentVersion,
        contentScope: RestorableLibraryScope,
        selectedViewID: String?
    ) {
        self.version = version
        self.contentScope = contentScope
        self.selectedViewID = selectedViewID
    }

    public static let initial = MacPresentationSnapshot(
        contentScope: .library,
        selectedViewID: nil
    )

    public func validated(against playlists: [PlaylistItem]) -> MacPresentationSnapshot {
        guard case .playlist(let playlistID) = contentScope else {
            return self
        }
        guard playlists.contains(where: { $0.id == playlistID }) else {
            return MacPresentationSnapshot(
                contentScope: .library,
                selectedViewID: selectedViewID
            )
        }
        return self
    }
}

public enum MacPresentationPersistence {
    public static let fallbackKey = "ContentView.lastSession.v1"

    public static func encode(_ snapshot: MacPresentationSnapshot) -> String? {
        guard let data = try? JSONEncoder().encode(snapshot) else {
            return nil
        }
        return data.base64EncodedString()
    }

    public static func decode(_ encoded: String?) -> MacPresentationSnapshot? {
        guard let encoded,
              let data = Data(base64Encoded: encoded),
              let snapshot = try? JSONDecoder().decode(MacPresentationSnapshot.self, from: data),
              snapshot.version == MacPresentationSnapshot.currentVersion
        else {
            return nil
        }
        return snapshot
    }

    public static func load(defaults: UserDefaults = .standard) -> MacPresentationSnapshot? {
        decode(defaults.string(forKey: fallbackKey))
    }

    public static func save(
        _ snapshot: MacPresentationSnapshot,
        defaults: UserDefaults = .standard
    ) {
        guard let encoded = encode(snapshot) else {
            return
        }
        defaults.set(encoded, forKey: fallbackKey)
    }

    public static func clear(defaults: UserDefaults = .standard) {
        defaults.removeObject(forKey: fallbackKey)
    }
}
