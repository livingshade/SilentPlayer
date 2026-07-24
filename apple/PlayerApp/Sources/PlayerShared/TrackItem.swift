import Foundation

public struct TrackItem: Identifiable, Hashable, Sendable {
    public let id: String
    public let viewID: String
    public let primaryViewID: String
    public let isPrimaryView: Bool
    public let viewKind: String
    public let viewName: String?
    public let rating: Int?
    public let title: String
    public let artist: String
    public let album: String
    public let durationMS: Int?
    public let artworkCount: Int
    public let artworkURL: URL?
    public let artworkSource: String?
    public let defaultViewPriority: Int
    public let hasAlbumIdentity: Bool
    public let path: String
    public let qualityProfile: String?
    public let formatName: String?
    public let gainDB: Double?
    public let loudnessStatus: String

    public init(
        id: String,
        viewID: String? = nil,
        primaryViewID: String? = nil,
        isPrimaryView: Bool = true,
        viewKind: String = "primary",
        viewName: String? = nil,
        rating: Int? = nil,
        title: String,
        artist: String,
        album: String = "",
        durationMS: Int?,
        artworkCount: Int = 0,
        artworkURL: URL? = nil,
        artworkSource: String? = nil,
        defaultViewPriority: Int? = nil,
        hasAlbumIdentity: Bool = false,
        path: String,
        qualityProfile: String? = nil,
        formatName: String? = nil,
        gainDB: Double? = nil,
        loudnessStatus: String = "NeedsAnalysis"
    ) {
        self.id = id
        self.viewID = viewID ?? id
        self.primaryViewID = primaryViewID ?? id
        self.isPrimaryView = isPrimaryView
        self.viewKind = viewKind
        self.viewName = MetadataDefaults.optional(viewName)
        self.rating = rating
        self.title = MetadataDefaults.title(title)
        self.artist = MetadataDefaults.artist(artist)
        self.album = MetadataDefaults.album(album)
        self.durationMS = durationMS
        self.artworkCount = artworkCount
        self.artworkURL = artworkURL
        self.artworkSource = artworkSource
        self.defaultViewPriority = defaultViewPriority
            ?? (artworkURL != nil ? 2 : (isPrimaryView ? 1 : 0))
        self.hasAlbumIdentity = hasAlbumIdentity
        self.path = path
        self.qualityProfile = qualityProfile
        self.formatName = formatName
        self.gainDB = gainDB
        self.loudnessStatus = loudnessStatus
    }

    public static func preferredDefaultView(in views: [TrackItem]) -> TrackItem? {
        guard let highestPriority = views.map(\.defaultViewPriority).max() else {
            return nil
        }
        return views.first { $0.defaultViewPriority == highestPriority }
    }

    public var durationText: String {
        guard let durationMS else {
            return "--:--"
        }

        let totalSeconds = max(0, durationMS / 1000)
        return "\(totalSeconds / 60):\(String(format: "%02d", totalSeconds % 60))"
    }

    public var subtitle: String {
        if !artist.isEmpty && !album.isEmpty {
            return "\(artist) - \(album)"
        }
        if !artist.isEmpty {
            return artist
        }
        if !album.isEmpty {
            return album
        }
        return path
    }

    public var gainText: String {
        guard let gainDB else {
            return loudnessStatus
        }
        return String(format: "%+.1f dB", gainDB)
    }

    public var ratingText: String {
        guard let rating else {
            return "Unrated"
        }
        return "\(rating)/10"
    }
}
