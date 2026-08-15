import Foundation

public enum LibraryScope: Hashable, Sendable {
    case library
    case favorites
    case history
    case playlist(String)

    public var title: String {
        switch self {
        case .library:
            return "Library"
        case .favorites:
            return "Favorites"
        case .history:
            return "History"
        case .playlist(let name):
            return name
        }
    }
}

public enum PlaylistSortMode: CaseIterable, Identifiable, Sendable {
    case defaultOrder
    case title
    case artist
    case album
    case rating

    public var id: String {
        apiValue
    }

    public var apiValue: String {
        switch self {
        case .defaultOrder:
            return "manual"
        case .title:
            return "title"
        case .artist:
            return "artist"
        case .album:
            return "album"
        case .rating:
            return "rating"
        }
    }

    public var label: String {
        switch self {
        case .defaultOrder:
            return "Default"
        case .title:
            return "Name"
        case .artist:
            return "Artist"
        case .album:
            return "Album"
        case .rating:
            return "Rating"
        }
    }

    public var systemImage: String {
        switch self {
        case .defaultOrder:
            return "line.3.horizontal"
        case .title:
            return "textformat"
        case .artist:
            return "person"
        case .album:
            return "opticaldisc"
        case .rating:
            return "star"
        }
    }
}

public enum PlaybackRepeatMode: String, CaseIterable, Codable, Identifiable, Sendable {
    case off
    case all
    case one

    public var id: String {
        rawValue
    }

    public var apiValue: String {
        rawValue
    }

    public var label: String {
        switch self {
        case .off:
            return "Order"
        case .all:
            return "Repeat All"
        case .one:
            return "Repeat One"
        }
    }

    public var systemImage: String {
        switch self {
        case .off:
            return "list.number"
        case .all:
            return "repeat"
        case .one:
            return "repeat.1"
        }
    }

    public var next: PlaybackRepeatMode {
        switch self {
        case .off:
            return .all
        case .all:
            return .one
        case .one:
            return .off
        }
    }
}

public enum PlaybackMode: String, CaseIterable, Codable, Identifiable, Sendable {
    case sequential
    case shuffle
    case repeatOne = "repeat_one"

    public var id: String {
        rawValue
    }

    public var apiValue: String {
        rawValue
    }

    public var label: String {
        switch self {
        case .sequential:
            return "Sequential"
        case .shuffle:
            return "Shuffle"
        case .repeatOne:
            return "Repeat One"
        }
    }

    public var systemImage: String {
        switch self {
        case .sequential:
            return "list.number"
        case .shuffle:
            return "shuffle"
        case .repeatOne:
            return "repeat.1"
        }
    }
}

enum PlaybackStatusText {
    static func afterTrackChange(isPlaying: Bool, title: String?) -> String {
        let trimmedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines)
        let trackName = trimmedTitle.flatMap { $0.isEmpty ? nil : $0 } ?? "track"
        return isPlaying ? "Playing \(trackName)" : "Paused at \(trackName)"
    }
}

enum PlaybackPollingPolicy {
    static let timerInterval: TimeInterval = 1
    static let timerTolerance: TimeInterval = 0.2

    static func shouldPoll(
        hasNowPlayingItem: Bool,
        isPlaying: Bool
    ) -> Bool {
        hasNowPlayingItem && isPlaying
    }
}

enum PlaybackPresentationClock {
    static func positionMS(
        basePositionMS: Int,
        baseUptime: TimeInterval,
        currentUptime: TimeInterval,
        isPlaying: Bool,
        durationMS: Int?
    ) -> Int {
        guard isPlaying else {
            return basePositionMS
        }
        let elapsedMS = max(0, Int((currentUptime - baseUptime) * 1_000))
        let estimated = basePositionMS.saturatingAdd(elapsedMS)
        guard let durationMS, durationMS > 0 else {
            return estimated
        }
        return min(estimated, durationMS)
    }
}

private extension Int {
    func saturatingAdd(_ other: Int) -> Int {
        let (value, overflow) = addingReportingOverflow(other)
        return overflow ? .max : value
    }
}

internal struct LibraryPresentationCache {
    var tracks: [TrackItem]
    var selectedTrackID: String?
}
