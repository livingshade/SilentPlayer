import Foundation

public enum RestorableLibraryScope: Codable, Equatable, Hashable, Sendable {
    case library
    case history
    case playlist(Int64)
}
