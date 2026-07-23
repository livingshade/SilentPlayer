import AppKit
import PlayerShared
import SwiftUI
import UniformTypeIdentifiers

@main
struct SilentApp: App {
    @NSApplicationDelegateAdaptor(SilentAppDelegate.self) private var appDelegate
    @StateObject private var model = AppModel()

    init() {
        NSApplication.shared.setActivationPolicy(.regular)
        NSApplication.shared.activate(ignoringOtherApps: true)
    }

    var body: some Scene {
        WindowGroup {
            ContentView(model: model) {
                await MacFolderPicker.chooseFolder()
            } chooseArtworkFile: {
                await MacFolderPicker.chooseFile(
                    title: "Choose artwork",
                    allowedFileTypes: ["jpg", "jpeg", "png", "webp", "gif"]
                )
            } chooseLyricsFile: {
                await MacFolderPicker.chooseFile(
                    title: "Choose lyrics",
                    allowedFileTypes: ["lrc", "txt", "lyrics"]
                )
            } chooseExportFile: { track in
                await MacFolderPicker.chooseExportDestination(for: track)
            } chooseLibraryExportPackage: {
                await MacFolderPicker.chooseLibraryExportDestination()
            } chooseLibraryImportPackage: {
                await MacFolderPicker.chooseLibraryPackage()
            }
            .onAppear {
                appDelegate.model = model
            }
        }
        .windowStyle(.titleBar)
        .commands {
            CommandGroup(replacing: .newItem) {}
        }
    }
}

@MainActor
final class SilentAppDelegate: NSObject, NSApplicationDelegate {
    weak var model: AppModel?

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        model?.shutdownForQuit()
        return .terminateNow
    }
}

enum MacFolderPicker {
    @MainActor
    static func chooseFolder() async -> URL? {
        let panel = NSOpenPanel()
        panel.title = "Choose a music folder"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = false
        return panel.runModal() == .OK ? panel.url : nil
    }

    @MainActor
    static func chooseFile(title: String, allowedFileTypes: [String]) async -> URL? {
        let panel = NSOpenPanel()
        panel.title = title
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = false
        let contentTypes = allowedFileTypes.compactMap { UTType(filenameExtension: $0) }
        if !contentTypes.isEmpty {
            panel.allowedContentTypes = contentTypes
        }
        return panel.runModal() == .OK ? panel.url : nil
    }

    @MainActor
    static func chooseExportDestination(for track: TrackItem) async -> URL? {
        let panel = NSSavePanel()
        panel.title = "Export View"
        panel.canCreateDirectories = true
        panel.nameFieldStringValue = defaultExportFileName(for: track)
        let fileExtension = exportExtension(for: track)
        if let contentType = UTType(filenameExtension: fileExtension) {
            panel.allowedContentTypes = [contentType]
        }
        return panel.runModal() == .OK ? panel.url : nil
    }

    @MainActor
    static func chooseLibraryExportDestination() async -> URL? {
        let panel = NSSavePanel()
        panel.title = "Export Library"
        panel.message = "Choose where to save the complete Silent library package."
        panel.prompt = "Export"
        panel.canCreateDirectories = true
        panel.nameFieldStringValue = defaultLibraryPackageName()
        return panel.runModal() == .OK ? panel.url : nil
    }

    @MainActor
    static func chooseLibraryPackage() async -> URL? {
        let panel = NSOpenPanel()
        panel.title = "Import Library"
        panel.message = "Choose a Silent library package. The current library will be backed up first."
        panel.prompt = "Import"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = false
        return panel.runModal() == .OK ? panel.url : nil
    }

    private static func defaultLibraryPackageName() -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return "Silent-Library-\(formatter.string(from: Date())).silentlibrary"
    }

    private static func defaultExportFileName(for track: TrackItem) -> String {
        let title = sanitizedFileComponent(track.title)
        let fileExtension = exportExtension(for: track)
        guard !fileExtension.isEmpty else {
            return title
        }
        return "\(title).\(fileExtension)"
    }

    private static func exportExtension(for track: TrackItem) -> String {
        if let formatName = track.formatName?.trimmingCharacters(in: .whitespacesAndNewlines),
           !formatName.isEmpty {
            return formatName.lowercased()
        }
        return URL(fileURLWithPath: track.path).pathExtension.lowercased()
    }

    private static func sanitizedFileComponent(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        let fallback = trimmed.isEmpty ? "Silent Export" : trimmed
        return fallback.replacingOccurrences(
            of: #"[/:]"#,
            with: "-",
            options: .regularExpression
        )
    }
}
