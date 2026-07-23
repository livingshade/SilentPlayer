#if os(macOS)
import Foundation

public enum LibraryWorkerOperation: Hashable, Sendable {
    case importFolder(URL)
    case audit

    var name: String {
        switch self {
        case .importFolder:
            return "import"
        case .audit:
            return "audit"
        }
    }
}

public struct LibraryWorkerEvent: Hashable, Sendable {
    public let name: String
    public let operation: String
    public let index: Int?
    public let total: Int?
    public let imported: Int?
    public let copied: Int?
    public let duplicatesSkipped: Int?
    public let artworkCached: Int?
    public let metadataWarnings: Int?
    public let tracksScanned: Int?
    public let hashesUpdated: Int?
    public let duplicateGroups: Int?
    public let tracksMerged: Int?
    public let failures: Int?
    public let title: String?
    public let path: String?
    public let reason: String?
    public let error: String?
}

public enum LibraryWorkerError: LocalizedError, Sendable {
    case executableMissing(String)

    public var errorDescription: String? {
        switch self {
        case .executableMissing(let path):
            return "Library worker not found at \(path)"
        }
    }
}

public final class LibraryWorker: @unchecked Sendable {
    private let operation: LibraryWorkerOperation
    private let dbURL: URL
    private let mediaRootURL: URL
    private let repoRoot: URL
    private let onEvent: @Sendable (LibraryWorkerEvent) -> Void
    private let onExit: @Sendable (Int32) -> Void
    private let process = Process()
    private let stdout = Pipe()
    private let stderr = Pipe()
    private let parseQueue = DispatchQueue(label: "normalplayer.library-worker.parse")
    private let decoder: JSONDecoder
    private var stdoutBuffer = ""
    private var didStop = false

    public init(
        operation: LibraryWorkerOperation,
        dbURL: URL,
        mediaRootURL: URL,
        repoRoot: URL,
        onEvent: @escaping @Sendable (LibraryWorkerEvent) -> Void,
        onExit: @escaping @Sendable (Int32) -> Void
    ) {
        self.operation = operation
        self.dbURL = dbURL
        self.mediaRootURL = mediaRootURL
        self.repoRoot = repoRoot
        self.onEvent = onEvent
        self.onExit = onExit
        self.decoder = JSONDecoder()
        self.decoder.keyDecodingStrategy = .convertFromSnakeCase
    }

    deinit {
        stop()
    }

    public var isRunning: Bool {
        process.isRunning
    }

    public func start() throws {
        let executable = try workerExecutableURL()
        process.executableURL = executable
        process.arguments = arguments()
        process.standardOutput = stdout
        process.standardError = stderr

        stdout.fileHandleForReading.readabilityHandler = { [weak self] handle in
            self?.consumeStdout(handle.availableData)
        }
        stderr.fileHandleForReading.readabilityHandler = { [weak self] handle in
            self?.consumeStderr(handle.availableData)
        }
        process.terminationHandler = { [weak self] process in
            self?.finish(exitCode: process.terminationStatus)
        }

        try process.run()
    }

    public func stop() {
        didStop = true
        stdout.fileHandleForReading.readabilityHandler = nil
        stderr.fileHandleForReading.readabilityHandler = nil
        if process.isRunning {
            process.terminate()
        }
    }

    private func arguments() -> [String] {
        switch operation {
        case .importFolder(let folder):
            return [
                "import",
                "--db", dbURL.path,
                "--media-root", mediaRootURL.path,
                "--folder", folder.path
            ]
        case .audit:
            return ["audit", "--db", dbURL.path]
        }
    }

    private func workerExecutableURL() throws -> URL {
        if let envPath = ProcessInfo.processInfo.environment["PLAYER_LIBRARY_WORKER"] {
            let url = URL(fileURLWithPath: envPath)
            if FileManager.default.isExecutableFile(atPath: url.path) {
                return url
            }
        }

        if let executableDir = Bundle.main.executableURL?.deletingLastPathComponent() {
            let bundled = executableDir.appendingPathComponent("player_library_worker")
            if FileManager.default.isExecutableFile(atPath: bundled.path) {
                return bundled
            }
        }

        let debugBuild = repoRoot
            .appendingPathComponent("target")
            .appendingPathComponent("debug")
            .appendingPathComponent("player_library_worker")
        if FileManager.default.isExecutableFile(atPath: debugBuild.path) {
            return debugBuild
        }

        throw LibraryWorkerError.executableMissing(debugBuild.path)
    }

    private func consumeStdout(_ data: Data) {
        guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else {
            return
        }

        parseQueue.async { [weak self] in
            guard let self else { return }
            self.stdoutBuffer += text
            let parts = self.stdoutBuffer.split(separator: "\n", omittingEmptySubsequences: false)
            self.stdoutBuffer = parts.last.map(String.init) ?? ""
            for line in parts.dropLast() {
                self.decodeLine(String(line))
            }
        }
    }

    private func consumeStderr(_ data: Data) {
        guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else {
            return
        }

        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return
        }
        onEvent(LibraryWorkerEvent(
            name: "stderr",
            operation: operation.name,
            index: nil,
            total: nil,
            imported: nil,
            copied: nil,
            duplicatesSkipped: nil,
            artworkCached: nil,
            metadataWarnings: nil,
            tracksScanned: nil,
            hashesUpdated: nil,
            duplicateGroups: nil,
            tracksMerged: nil,
            failures: nil,
            title: nil,
            path: nil,
            reason: nil,
            error: trimmed
        ))
    }

    private func decodeLine(_ line: String) {
        guard !line.isEmpty else {
            return
        }

        do {
            let event = try decoder.decode(LibraryWorkerEventDTO.self, from: Data(line.utf8))
            onEvent(event.model)
        } catch {
            onEvent(LibraryWorkerEvent(
                name: "decode_error",
                operation: operation.name,
                index: nil,
                total: nil,
                imported: nil,
                copied: nil,
                duplicatesSkipped: nil,
                artworkCached: nil,
                metadataWarnings: nil,
                tracksScanned: nil,
                hashesUpdated: nil,
                duplicateGroups: nil,
                tracksMerged: nil,
                failures: nil,
                title: nil,
                path: nil,
                reason: nil,
                error: "\(error.localizedDescription): \(line)"
            ))
        }
    }

    private func finish(exitCode: Int32) {
        stdout.fileHandleForReading.readabilityHandler = nil
        stderr.fileHandleForReading.readabilityHandler = nil
        parseQueue.async { [weak self] in
            guard let self else { return }
            if !self.stdoutBuffer.isEmpty {
                self.decodeLine(self.stdoutBuffer)
                self.stdoutBuffer = ""
            }
            if !self.didStop {
                self.onExit(exitCode)
            }
        }
    }
}

private struct LibraryWorkerEventDTO: Decodable {
    let event: String
    let operation: String
    let index: Int?
    let total: Int?
    let imported: Int?
    let copied: Int?
    let duplicatesSkipped: Int?
    let artworkCached: Int?
    let metadataWarnings: Int?
    let tracksScanned: Int?
    let hashesUpdated: Int?
    let duplicateGroups: Int?
    let tracksMerged: Int?
    let failures: Int?
    let title: String?
    let path: String?
    let reason: String?
    let error: String?

    var model: LibraryWorkerEvent {
        LibraryWorkerEvent(
            name: event,
            operation: operation,
            index: index,
            total: total,
            imported: imported,
            copied: copied,
            duplicatesSkipped: duplicatesSkipped,
            artworkCached: artworkCached,
            metadataWarnings: metadataWarnings,
            tracksScanned: tracksScanned,
            hashesUpdated: hashesUpdated,
            duplicateGroups: duplicateGroups,
            tracksMerged: tracksMerged,
            failures: failures,
            title: title,
            path: path,
            reason: reason,
            error: error
        )
    }
}
#endif
