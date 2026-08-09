#if os(macOS)
import Foundation

public enum AnalyzerWorkerEvent: Hashable, Sendable {
    case started(total: Int)
    case trackFinished(index: Int, total: Int, title: String, analyzed: Int, failed: Int)
    case trackFailed(index: Int, total: Int, title: String, error: String)
    case albumFinished(albumsAnalyzed: Int, tracksUpdated: Int)
    case finished(analyzed: Int, failed: Int, albumsAnalyzed: Int)
    case fatal(String)
    case stderr(String)
    case protocolError(String)
}

public enum AnalyzerWorkerError: LocalizedError, Sendable {
    case executableMissing(String)

    public var errorDescription: String? {
        switch self {
        case .executableMissing(let path):
            return "Analyzer worker not found at \(path)"
        }
    }
}

public final class AnalyzerWorker: @unchecked Sendable {
    private let dbURL: URL
    private let repoRoot: URL
    private let onEvent: @Sendable (AnalyzerWorkerEvent) -> Void
    private let onExit: @Sendable (Int32) -> Void
    private let process = Process()
    private let stdout = Pipe()
    private let stderr = Pipe()
    private let parseQueue = DispatchQueue(label: "normalplayer.analyzer-worker.parse")
    private let decoder: JSONDecoder
    private var stdoutBuffer = ""
    private var didStop = false

    public init(
        dbURL: URL,
        repoRoot: URL,
        onEvent: @escaping @Sendable (AnalyzerWorkerEvent) -> Void,
        onExit: @escaping @Sendable (Int32) -> Void
    ) {
        self.dbURL = dbURL
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
        let executable = try analyzerExecutableURL()
        process.executableURL = executable
        process.arguments = ["--db", dbURL.path]
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

    private func analyzerExecutableURL() throws -> URL {
        if let envPath = ProcessInfo.processInfo.environment["PLAYER_ANALYZER"] {
            let url = URL(fileURLWithPath: envPath)
            if FileManager.default.isExecutableFile(atPath: url.path) {
                return url
            }
        }

        if let executableDir = Bundle.main.executableURL?.deletingLastPathComponent() {
            let bundled = executableDir.appendingPathComponent("analyzer")
            if FileManager.default.isExecutableFile(atPath: bundled.path) {
                return bundled
            }
        }

        let debugBuild = repoRoot
            .appendingPathComponent("target")
            .appendingPathComponent("debug")
            .appendingPathComponent("analyzer")
        if FileManager.default.isExecutableFile(atPath: debugBuild.path) {
            return debugBuild
        }

        throw AnalyzerWorkerError.executableMissing(debugBuild.path)
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
            let event = try decoder.decode(AnalyzerWorkerEventDTO.self, from: Data(line.utf8))
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

struct AnalyzerWorkerEventDTO: Decodable {
    let event: String
    let index: Int?
    let total: Int?
    let analyzed: Int?
    let failed: Int?
    let title: String?
    let path: String?
    let error: String?
    let albumsAnalyzed: Int?
    let albumTracksUpdated: Int?
    let albumSkipped: Int?

    func model() throws -> AnalyzerWorkerEvent {
        switch event {
        case "started":
            return .started(total: try required(total, "total"))
        case "track_finished":
            return .trackFinished(
                index: try required(index, "index"),
                total: try required(total, "total"),
                title: try required(title, "title"),
                analyzed: try required(analyzed, "analyzed"),
                failed: try required(failed, "failed")
            )
        case "track_failed":
            return .trackFailed(
                index: try required(index, "index"),
                total: try required(total, "total"),
                title: try required(title, "title"),
                error: try required(error, "error")
            )
        case "album_finished":
            return .albumFinished(
                albumsAnalyzed: try required(albumsAnalyzed, "albums_analyzed"),
                tracksUpdated: try required(albumTracksUpdated, "album_tracks_updated")
            )
        case "finished":
            return .finished(
                analyzed: try required(analyzed, "analyzed"),
                failed: try required(failed, "failed"),
                albumsAnalyzed: try required(albumsAnalyzed, "albums_analyzed")
            )
        case "fatal":
            return .fatal(try required(error, "error"))
        default:
            throw WorkerProtocolError.unknownEvent(event)
        }
    }
}

private enum WorkerProtocolError: LocalizedError {
    case missingField(event: String, field: String)
    case unknownEvent(String)

    var errorDescription: String? {
        switch self {
        case .missingField(let event, let field):
            return "Analyzer event '\(event)' is missing '\(field)'"
        case .unknownEvent(let event):
            return "Unknown analyzer event '\(event)'"
        }
    }
}

private extension AnalyzerWorkerEventDTO {
    func required<T>(_ value: T?, _ field: String) throws -> T {
        guard let value else {
            throw WorkerProtocolError.missingField(event: event, field: field)
        }
        return value
    }
}
#endif
