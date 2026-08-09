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

public enum LibraryWorkerTask: String, Hashable, Sendable {
    case `import`
    case audit
}

public enum LibraryWorkerEvent: Hashable, Sendable {
    case started(operation: LibraryWorkerTask, total: Int)
    case trackFinished(operation: LibraryWorkerTask, index: Int, total: Int, title: String,
        imported: Int, copied: Int, duplicatesSkipped: Int, artworkCached: Int,
        metadataWarnings: Int, failures: Int)
    case trackSkipped(operation: LibraryWorkerTask, index: Int, total: Int, title: String,
        reason: String, duplicatesSkipped: Int, failures: Int)
    case trackFailed(operation: LibraryWorkerTask, index: Int, total: Int, title: String?, error: String)
    case mergeFinished(duplicateGroups: Int, tracksMerged: Int, failures: Int)
    case importFinished(imported: Int, copied: Int, duplicatesSkipped: Int,
        artworkCached: Int, metadataWarnings: Int, failures: Int)
    case auditFinished(tracksScanned: Int, hashesUpdated: Int, duplicateGroups: Int,
        tracksMerged: Int, failures: Int)
    case fatal(String)
    case stderr(String)
    case protocolError(String)
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
            let bundled = executableDir.appendingPathComponent("library_worker")
            if FileManager.default.isExecutableFile(atPath: bundled.path) {
                return bundled
            }
        }

        let debugBuild = repoRoot
            .appendingPathComponent("target")
            .appendingPathComponent("debug")
            .appendingPathComponent("library_worker")
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
        onEvent(.stderr(trimmed))
    }

    private func decodeLine(_ line: String) {
        guard !line.isEmpty else {
            return
        }

        do {
            let event = try decoder.decode(LibraryWorkerEventDTO.self, from: Data(line.utf8))
            onEvent(try event.model())
        } catch {
            onEvent(.protocolError("\(error.localizedDescription): \(line)"))
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

struct LibraryWorkerEventDTO: Decodable {
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

    func model() throws -> LibraryWorkerEvent {
        let task = try LibraryWorkerTask(rawValue: operation).unwrap(
            or: LibraryWorkerProtocolError.invalidOperation(operation)
        )
        switch event {
        case "started":
            return .started(operation: task, total: try required(total, "total"))
        case "track_finished":
            return .trackFinished(
                operation: task,
                index: try required(index, "index"),
                total: try required(total, "total"),
                title: try required(title, "title"),
                imported: try required(imported, "imported"),
                copied: try required(copied, "copied"),
                duplicatesSkipped: try required(duplicatesSkipped, "duplicates_skipped"),
                artworkCached: try required(artworkCached, "artwork_cached"),
                metadataWarnings: try required(metadataWarnings, "metadata_warnings"),
                failures: try required(failures, "failures")
            )
        case "track_skipped":
            return .trackSkipped(
                operation: task,
                index: try required(index, "index"),
                total: try required(total, "total"),
                title: try required(title, "title"),
                reason: try required(reason, "reason"),
                duplicatesSkipped: try required(duplicatesSkipped, "duplicates_skipped"),
                failures: try required(failures, "failures")
            )
        case "track_failed":
            return .trackFailed(
                operation: task,
                index: try required(index, "index"),
                total: try required(total, "total"),
                title: title,
                error: try required(error, "error")
            )
        case "merge_finished":
            return .mergeFinished(
                duplicateGroups: try required(duplicateGroups, "duplicate_groups"),
                tracksMerged: try required(tracksMerged, "tracks_merged"),
                failures: try required(failures, "failures")
            )
        case "finished" where task == .import:
            return .importFinished(
                imported: try required(imported, "imported"),
                copied: try required(copied, "copied"),
                duplicatesSkipped: try required(duplicatesSkipped, "duplicates_skipped"),
                artworkCached: try required(artworkCached, "artwork_cached"),
                metadataWarnings: try required(metadataWarnings, "metadata_warnings"),
                failures: try required(failures, "failures")
            )
        case "finished":
            return .auditFinished(
                tracksScanned: try required(tracksScanned, "tracks_scanned"),
                hashesUpdated: try required(hashesUpdated, "hashes_updated"),
                duplicateGroups: try required(duplicateGroups, "duplicate_groups"),
                tracksMerged: try required(tracksMerged, "tracks_merged"),
                failures: try required(failures, "failures")
            )
        case "fatal":
            return .fatal(try required(error, "error"))
        default:
            throw LibraryWorkerProtocolError.unknownEvent(event)
        }
    }
}

private enum LibraryWorkerProtocolError: LocalizedError {
    case missingField(event: String, field: String)
    case invalidOperation(String)
    case unknownEvent(String)

    var errorDescription: String? {
        switch self {
        case .missingField(let event, let field):
            return "Library worker event '\(event)' is missing '\(field)'"
        case .invalidOperation(let operation):
            return "Unknown library worker operation '\(operation)'"
        case .unknownEvent(let event):
            return "Unknown library worker event '\(event)'"
        }
    }
}

private extension LibraryWorkerEventDTO {
    func required<T>(_ value: T?, _ field: String) throws -> T {
        guard let value else {
            throw LibraryWorkerProtocolError.missingField(event: event, field: field)
        }
        return value
    }
}

private extension Optional {
    func unwrap(or error: @autoclosure () -> Error) throws -> Wrapped {
        guard let value = self else { throw error() }
        return value
    }
}
#endif
