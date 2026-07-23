#if os(macOS)
import Foundation

public struct AnalyzerWorkerEvent: Hashable, Sendable {
    public let name: String
    public let index: Int?
    public let total: Int?
    public let analyzed: Int?
    public let failed: Int?
    public let title: String?
    public let path: String?
    public let error: String?
    public let albumsAnalyzed: Int?
    public let albumTracksUpdated: Int?
    public let albumSkipped: Int?
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
            let bundled = executableDir.appendingPathComponent("player_analyzer")
            if FileManager.default.isExecutableFile(atPath: bundled.path) {
                return bundled
            }
        }

        let debugBuild = repoRoot
            .appendingPathComponent("target")
            .appendingPathComponent("debug")
            .appendingPathComponent("player_analyzer")
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
        onEvent(AnalyzerWorkerEvent(
            name: "stderr",
            index: nil,
            total: nil,
            analyzed: nil,
            failed: nil,
            title: nil,
            path: nil,
            error: trimmed,
            albumsAnalyzed: nil,
            albumTracksUpdated: nil,
            albumSkipped: nil
        ))
    }

    private func decodeLine(_ line: String) {
        guard !line.isEmpty else {
            return
        }

        do {
            let event = try decoder.decode(AnalyzerWorkerEventDTO.self, from: Data(line.utf8))
            onEvent(event.model)
        } catch {
            onEvent(AnalyzerWorkerEvent(
                name: "decode_error",
                index: nil,
                total: nil,
                analyzed: nil,
                failed: nil,
                title: nil,
                path: nil,
                error: "\(error.localizedDescription): \(line)",
                albumsAnalyzed: nil,
                albumTracksUpdated: nil,
                albumSkipped: nil
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

private struct AnalyzerWorkerEventDTO: Decodable {
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

    var model: AnalyzerWorkerEvent {
        AnalyzerWorkerEvent(
            name: event,
            index: index,
            total: total,
            analyzed: analyzed,
            failed: failed,
            title: title,
            path: path,
            error: error,
            albumsAnalyzed: albumsAnalyzed,
            albumTracksUpdated: albumTracksUpdated,
            albumSkipped: albumSkipped
        )
    }
}
#endif
