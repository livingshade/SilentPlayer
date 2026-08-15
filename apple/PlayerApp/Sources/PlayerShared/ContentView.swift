#if os(macOS)
import Foundation
import SwiftUI

public struct ContentView: View {
    @ObservedObject internal var model: AppModel
    @Environment(\.scenePhase) internal var scenePhase
    @SceneStorage("ContentView.sceneSession.v1") internal var sceneSession = ""
    @State internal var isRestoringPresentation = true
    @State internal var pendingSeekProgress: Double?
    @State internal var pendingSingleClick: DispatchWorkItem?
    @State internal var isFileChecksExpanded = false
    @State internal var isZeroOutConfirmationPresented = false
    @State internal var isQueuePresented = false
    @State internal var isLibraryInformationPresented = false
    @State internal var isNowPlayingExpanded = false
    @State internal var splitViewVisibility: NavigationSplitViewVisibility = .all
    @State internal var pendingLibraryDeletion: TrackItem?
    @State internal var isLibraryDeletionConfirmationPresented = false
    internal let chooseFolder: () async -> URL?
    internal let chooseArtworkFile: () async -> URL?
    internal let chooseLyricsFile: () async -> URL?
    internal let chooseExportFile: (TrackItem) async -> URL?
    internal let chooseLibraryExportPackage: () async -> URL?
    internal let chooseLibraryImportPackage: () async -> URL?

    public init(
        model: AppModel,
        chooseFolder: @escaping () async -> URL?,
        chooseArtworkFile: @escaping () async -> URL?,
        chooseLyricsFile: @escaping () async -> URL?,
        chooseExportFile: @escaping (TrackItem) async -> URL?,
        chooseLibraryExportPackage: @escaping () async -> URL?,
        chooseLibraryImportPackage: @escaping () async -> URL?
    ) {
        self.model = model
        self.chooseFolder = chooseFolder
        self.chooseArtworkFile = chooseArtworkFile
        self.chooseLyricsFile = chooseLyricsFile
        self.chooseExportFile = chooseExportFile
        self.chooseLibraryExportPackage = chooseLibraryExportPackage
        self.chooseLibraryImportPackage = chooseLibraryImportPackage
    }

    public var body: some View {
        Group {
            if isRestoringPresentation {
                restorationPlaceholder
            } else {
                playerContent
            }
        }
        .frame(minWidth: 960, idealWidth: 1180, minHeight: 620, idealHeight: 780)
        .sheet(isPresented: featureBinding(model.trackDetail, \.isEditPresented)) {
            TrackEditSheet(
                model: model,
                chooseArtworkFile: chooseArtworkFile,
                chooseLyricsFile: chooseLyricsFile
            )
        }
        .sheet(isPresented: Binding(
            get: { model.playlists.presentedSheet != nil },
            set: { isPresented in
                if !isPresented {
                    model.dismissPlaylistSheet()
                }
            }
        )) {
            MacPlaylistSheetHost(
                model: model,
                chooseArtworkFile: chooseArtworkFile
            )
        }
        .sheet(isPresented: $isQueuePresented) {
            PlaybackQueueSheet(model: model)
        }
        .sheet(isPresented: $isLibraryInformationPresented) {
            LibraryInformationSheet(
                status: model.operations.status,
                databasePath: model.dbPath,
                musicPath: model.mediaRootPath
            )
        }
        .confirmationDialog(
            "Zero Out Library?",
            isPresented: $isZeroOutConfirmationPresented,
            titleVisibility: .visible
        ) {
            Button("Zero Out Library", role: .destructive) {
                Task { await model.zeroOutLibrary() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This permanently deletes the current database and managed music files. No internal backup will be created.")
        }
        .confirmationDialog(
            "Delete Song from Library?",
            isPresented: $isLibraryDeletionConfirmationPresented,
            titleVisibility: .visible
        ) {
            Button("Delete from Library", role: .destructive) {
                guard let track = pendingLibraryDeletion else {
                    return
                }
                pendingLibraryDeletion = nil
                Task { await model.deleteFromLibrary(track) }
            }
            Button("Cancel", role: .cancel) {
                pendingLibraryDeletion = nil
            }
        } message: {
            if let track = pendingLibraryDeletion {
                Text("“\(track.title)” will be removed from Library, every playlist, favorites, history, and the managed music folder. This can’t be undone.")
            }
        }
        .task {
            await restorePresentation()
        }
        .onChange(of: model.library.scope) { _ in
            persistPresentation()
        }
        .onChange(of: model.library.selectedTrack?.id) { _ in
            persistPresentation()
        }
        .onChange(of: model.playback.nowPlaying?.id) { trackID in
            pendingSeekProgress = nil
            if trackID == nil {
                dismissExpandedNowPlaying()
            }
        }
        .onChange(of: model.playlists.items) { _ in
            persistPresentation()
        }
        .onChange(of: scenePhase) { phase in
            if phase == .background {
                persistPresentation()
            }
        }
        .onDisappear {
            persistPresentation()
        }
    }
}
#endif
