#if os(iOS)
import Foundation
import SwiftUI

private enum PhoneRoute: Hashable {
    case playlist(Int64)

    var playlistID: Int64 {
        switch self {
        case .playlist(let id):
            return id
        }
    }
}

public struct PhoneContentView: View {
    @ObservedObject private var model: AppModel
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.scenePhase) private var scenePhase
    @SceneStorage("PhoneContentView.sceneSession.v1") private var sceneSession = ""
    @State private var selectedTab: PhonePresentationTab = .library
    @State private var playlistPath: [PhoneRoute] = []
    @State private var isRestoringPresentation = true
    @State private var fileImportPurpose: PhoneFileImportPurpose?
    @State private var isFileImporterPresented = false
    @State private var pendingLibraryExportURL: URL?
    @State private var isLibraryExporterPresented = false
    @State private var isZeroOutConfirmationPresented = false
    @State private var pendingSeekProgress: Double?
    @State private var lastExportURL: URL?
    @State private var activeAlert: PhoneAppAlert?
    @State private var isNowPlayingPresented = false
    @State private var isLyricsPresented = false
    @State private var isQueuePresented = false
    @State private var pendingLibraryDeletion: TrackItem?
    @State private var isLibraryDeletionConfirmationPresented = false

    public init(model: AppModel) {
        self.model = model
    }

    public var body: some View {
        ZStack {
            if let startupError = model.startupError {
                startupFailureView(message: startupError)
            } else {
                TabView(selection: $selectedTab) {
                    libraryTab
                        .tabItem {
                            Label("Library", systemImage: "music.note.list")
                        }
                        .tag(PhonePresentationTab.library)

                    playlistsTab
                        .tabItem {
                            Label("Playlists", systemImage: "music.note.house")
                        }
                        .tag(PhonePresentationTab.playlists)
                }
            }

            if model.operations.isBusy && !isRestoringPresentation {
                busyOverlay
            }
        }
        .background(
            PhoneDocumentPickerBridge(
                isPresented: $isFileImporterPresented,
                purpose: fileImportPurpose,
                onResult: handleFileImport
            )
            .frame(width: 0, height: 0)
        )
        .background(
            PhoneDocumentExporterBridge(
                isPresented: $isLibraryExporterPresented,
                sourceURL: pendingLibraryExportURL,
                onResult: handleLibraryExport
            )
            .frame(width: 0, height: 0)
        )
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
        .fullScreenCover(isPresented: $isNowPlayingPresented) {
            nowPlayingView
        }
        .sheet(isPresented: Binding(
            get: { model.playlists.presentedSheet != nil },
            set: { isPresented in
                if !isPresented {
                    model.dismissPlaylistSheet()
                }
            }
        )) {
            PhonePlaylistSheetHost(model: model) {
                presentFileImporter(.playlistSettingsArtwork)
            }
        }
        .sheet(isPresented: featureBinding(model.trackDetail, \.isEditPresented)) {
            PhoneTrackEditSheet(
                model: model,
                chooseArtwork: { presentFileImporter(.editArtwork) },
                chooseLyrics: { presentFileImporter(.editLyrics) }
            )
        }
        .task {
            await restorePresentation()
        }
        .onChange(of: model.playback.error) { error in
            presentError(error)
        }
        .onChange(of: selectedTab) { tab in
            guard !isRestoringPresentation else {
                return
            }
            switch tab {
            case .library:
                Task {
                    await model.showLibrary()
                    persistPresentation()
                }
            case .playlists:
                Task {
                    await model.refreshPlaylists()
                    repairPlaylistPath()
                    persistPresentation()
                }
            }
        }
        .onChange(of: playlistPath) { _ in
            persistPresentation()
        }
        .onChange(of: model.library.scope) { _ in
            persistPresentation()
        }
        .onChange(of: model.library.selectedTrack?.id) { _ in
            persistPresentation()
        }
        .onChange(of: model.playlists.items) { _ in
            repairPlaylistPath()
            persistPresentation()
        }
        .onChange(of: scenePhase) { phase in
            if phase == .background {
                model.applicationDidEnterBackground()
                persistPresentation()
            }
        }
        .alert(item: $activeAlert) { alert in
            Alert(
                title: Text(alert.title),
                message: Text(alert.message),
                dismissButton: .default(Text("OK")) {
                    model.playback.error = ""
                }
            )
        }
    }

    private var libraryTab: some View {
        NavigationStack {
            libraryTrackList
                .navigationTitle("Library")
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) {
                        Menu {
                            Button {
                                Task { await model.showLibrary() }
                            } label: {
                                Label("Library", systemImage: "music.note.list")
                            }

                            Button {
                                Task { await model.showHistory() }
                            } label: {
                                Label("History", systemImage: "clock")
                            }
                        } label: {
                            Label(model.library.scope.title, systemImage: "line.3.horizontal")
                        }
                    }

                    ToolbarItemGroup(placement: .topBarTrailing) {
                        Button {
                            Task { await model.playEntireLibrary() }
                        } label: {
                            Label("Play All \(model.library.scope.title)", systemImage: "play.fill")
                        }
                        .disabled(model.library.tracks.isEmpty || model.operations.isBusy)

                        libraryActionsMenu
                    }
                }
                .safeAreaInset(edge: .bottom) {
                    miniPlayerBar
                }
        }
    }

    @ViewBuilder
    private var libraryTrackList: some View {
        if model.library.scope == .library {
            trackList(scopeTitle: model.library.scope.title)
                .searchable(text: librarySearchBinding, prompt: "Search songs")
                .onSubmit(of: .search) {
                    Task { await model.search() }
                }
        } else {
            trackList(scopeTitle: model.library.scope.title)
        }
    }

    private var librarySearchBinding: Binding<String> {
        Binding(
            get: { model.library.query },
            set: { newValue in
                let clearedSearch = !model.library.query.isEmpty && newValue.isEmpty
                model.library.query = newValue
                if clearedSearch {
                    Task { await model.reloadActiveScope() }
                }
            }
        )
    }

    private var playlistsTab: some View {
        NavigationStack(path: $playlistPath) {
            List {
                ForEach(model.playlists.items) { playlist in
                    phonePlaylistLink(playlist)
                }
            }
            .listStyle(.plain)
            .overlay {
                if isRestoringPresentation {
                    loadingPlaceholder("Loading Playlists")
                } else if model.playlists.items.isEmpty {
                    playlistEmptyState
                }
            }
            .navigationTitle("Playlists")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        model.presentCreatePlaylist()
                    } label: {
                        Label("New Playlist", systemImage: "plus")
                    }
                }
            }
            .refreshable {
                await model.refreshPlaylists()
            }
            .navigationDestination(for: PhoneRoute.self) { route in
                switch route {
                case .playlist(let id):
                    if let playlist = model.playlists.items.first(where: { $0.id == id }) {
                        PhonePlaylistDetailView(
                            model: model,
                            playlist: playlist,
                            confirmLibraryDeletion: presentLibraryDeletion
                        )
                    } else {
                        PhoneEmptyState(
                            title: "Playlist Unavailable",
                            message: "This playlist may have been removed.",
                            systemImage: "music.note.slash"
                        )
                        .navigationTitle("Playlist")
                        .navigationBarTitleDisplayMode(.inline)
                    }
                }
            }
        }
        .safeAreaInset(edge: .bottom) {
            miniPlayerBar
        }
    }

    private var playlistEmptyState: some View {
        VStack(spacing: 14) {
            Image(systemName: "music.note.house")
                .font(.system(size: 46))
                .foregroundStyle(.secondary)
            VStack(spacing: 5) {
                Text("No Playlists")
                    .font(.headline)
                Text("Create a playlist to organize songs for any mood or moment.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            Button {
                model.presentCreatePlaylist()
            } label: {
                Label("New Playlist", systemImage: "plus")
                    .frame(minWidth: 150)
            }
            .buttonStyle(.borderedProminent)
        }
        .padding(24)
        .frame(maxWidth: 360)
    }

    private func phonePlaylistLink(_ playlist: PlaylistItem) -> some View {
        NavigationLink(value: PhoneRoute.playlist(playlist.id)) {
            HStack(spacing: 12) {
                PhoneArtworkImage(
                    artworkURL: playlist.artworkURL,
                    placeholderSystemImage: "music.note.house",
                    size: 42,
                    cornerRadius: 8
                )
                VStack(alignment: .leading, spacing: 3) {
                    Text(playlist.name.phoneCompacted)
                        .font(.body.weight(.medium))
                        .fixedSize(horizontal: false, vertical: true)
                    Text("\(playlist.trackCount) songs")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .contextMenu {
            Button {
                Task { await model.playPlaylist(playlist, shuffled: false) }
            } label: {
                Label("Play in Order", systemImage: "play.fill")
            }

            Button {
                Task { await model.playPlaylist(playlist, shuffled: true) }
            } label: {
                Label("Shuffle", systemImage: "shuffle")
            }

            Button {
                model.presentPlaylistSettings(playlist)
            } label: {
                Label("Edit Playlist", systemImage: "pencil")
            }
        }
    }

    @MainActor
    private func restorePresentation() async {
        let requestedSnapshot = PhonePresentationPersistence.decode(sceneSession) ?? .initial

        selectedTab = requestedSnapshot.selectedTab
        await model.bootstrap(
            restoring: requestedSnapshot.bootstrapScope,
            preferredSelectedTrackID: requestedSnapshot.selectedTrackID
        )

        let validatedSnapshot = requestedSnapshot.validated(against: model.playlists.items)
        if validatedSnapshot.selectedTab == .playlists,
           let playlistID = validatedSnapshot.playlistDetailID {
            playlistPath = [.playlist(playlistID)]
        } else {
            playlistPath = []
        }

        isRestoringPresentation = false
        persistPresentation()
        presentError(model.playback.error)
    }

    private func persistPresentation() {
        guard !isRestoringPresentation else {
            return
        }

        let snapshot = PhonePresentationSnapshot(
            selectedTab: selectedTab,
            contentScope: presentationScope,
            playlistDetailID: playlistPath.last?.playlistID,
            selectedTrackID: model.library.selectedTrack?.id
        )
        guard let encoded = PhonePresentationPersistence.encode(snapshot) else {
            return
        }
        sceneSession = encoded
    }

    private var presentationScope: PhonePresentationScope {
        switch model.restorableLibraryScope {
        case .library:
            return .library
        case .history:
            return .history
        case .playlist(let id):
            return .playlist(id)
        }
    }

    private func repairPlaylistPath() {
        guard let route = playlistPath.last,
              !model.playlists.items.contains(where: { $0.id == route.playlistID }) else {
            return
        }
        playlistPath = []
    }

    private var nowPlayingView: some View {
        NavigationStack {
            GeometryReader { proxy in
                let availableArtworkWidth = max(0, proxy.size.width - 72)
                let regularArtworkSize = min(
                    availableArtworkWidth,
                    max(176, min(220, proxy.size.height * 0.36))
                )
                let compactArtworkSize = min(availableArtworkWidth, 168)

                Group {
                    if dynamicTypeSize.isAccessibilitySize {
                        ScrollView {
                            nowPlayingContent(
                                artworkSize: compactArtworkSize,
                                spacing: 10,
                                usesCompactControls: true
                            )
                            .padding(.vertical, 12)
                        }
                    } else {
                        ViewThatFits(in: .vertical) {
                            nowPlayingContent(
                                artworkSize: regularArtworkSize,
                                spacing: 16,
                                usesCompactControls: false
                            )
                            nowPlayingContent(
                                artworkSize: compactArtworkSize,
                                spacing: 10,
                                usesCompactControls: true
                            )
                        }
                        .padding(.vertical, 8)
                        .frame(maxHeight: .infinity)
                    }
                }
                .padding(.horizontal, 20)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .navigationTitle("Now Playing")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button {
                        isNowPlayingPresented = false
                    } label: {
                        Label("Close Now Playing", systemImage: "chevron.down")
                    }
                }

                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        isQueuePresented = true
                    } label: {
                        Label(model.queueStatusText, systemImage: "music.note.list")
                            .labelStyle(.titleAndIcon)
                            .font(.subheadline.weight(.semibold))
                    }
                }
            }
        }
        .sheet(isPresented: $isQueuePresented) {
            PhonePlaybackQueueSheet(model: model)
        }
        .fullScreenCover(isPresented: $isLyricsPresented) {
            PhoneNowPlayingLyricsView(model: model) {
                isLyricsPresented = false
            }
        }
    }

    private func nowPlayingContent(
        artworkSize: CGFloat,
        spacing: CGFloat,
        usesCompactControls: Bool
    ) -> some View {
        VStack(spacing: spacing) {
            let nowDetails = details(for: model.playback.nowPlaying)
            PhoneArtworkImage(
                artworkURL: nowDetails?.artworkURL ?? model.playback.nowPlaying?.artworkURL,
                placeholderSystemImage: "music.note",
                size: artworkSize,
                cornerRadius: 12
            )

            VStack(spacing: usesCompactControls ? 3 : 5) {
                Text(model.playback.nowPlaying?.phoneDisplayTitle ?? "Nothing Playing")
                    .font(
                        usesCompactControls
                            ? .headline
                            : .title3.weight(.semibold)
                    )
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .layoutPriority(1)
                Text(model.playback.nowPlaying?.phoneDisplaySubtitle ?? model.operations.status.phoneCompacted)
                    .font(usesCompactControls ? .footnote : .callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }

            playerControls(compact: usesCompactControls)

            Button {
                isLyricsPresented = true
            } label: {
                Label("Lyrics", systemImage: "text.quote")
                    .frame(minWidth: 132)
            }
            .buttonStyle(.bordered)
            .disabled(model.playback.nowPlaying == nil)
        }
        .frame(maxWidth: .infinity)
    }

    private func details(for track: TrackItem?) -> TrackDetails? {
        guard let track else {
            return nil
        }
        if let details = model.playback.details,
           details.identity == track.identity {
            return details
        }
        if let details = model.trackDetail.details,
           details.identity == track.identity {
            return details
        }
        return nil
    }

    private func trackList(scopeTitle: String) -> some View {
        List {
            ForEach(model.library.tracks) { track in
                Button {
                    play(track)
                } label: {
                    PhoneTrackRow(
                        track: track,
                        isCurrent: model.playback.nowPlaying?.id == track.id,
                        isPlaying: model.playback.nowPlaying?.id == track.id && model.playback.isPlaying
                    )
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Play \(track.phoneDisplayTitle)")
                .accessibilityHint("Adds this song to the global queue if needed, then starts it")
                .swipeActions(edge: .leading) {
                    Button {
                        play(track)
                    } label: {
                        Label("Play", systemImage: "play.fill")
                    }
                    .tint(.green)
                }
                .swipeActions(edge: .trailing) {
                    Button {
                        Task { await model.playNext(track) }
                    } label: {
                        Label("Play Next", systemImage: "text.line.first.and.arrowtriangle.forward")
                    }
                    .tint(.blue)
                }
                .contextMenu {
                    trackContextMenu(for: track)
                }
            }
        }
        .overlay {
            if isRestoringPresentation {
                loadingPlaceholder("Loading Library")
            } else if model.library.tracks.isEmpty,
                      model.library.scope == .library,
                      model.library.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                libraryImportEmptyState
            } else if model.library.tracks.isEmpty {
                PhoneEmptyState(
                    title: scopeTitle,
                    message: model.operations.status,
                    systemImage: emptyIcon
                )
            }
        }
    }

    private var libraryImportEmptyState: some View {
        VStack(spacing: 14) {
            Image(systemName: "music.note.list")
                .font(.system(size: 46))
                .foregroundStyle(.secondary)

            VStack(spacing: 5) {
                Text("Your Library Is Empty")
                    .font(.headline)
                Text("Import a Silent library, or add music stored on this iPhone or in Files.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }

            Button {
                presentFileImporter(.emptyLibraryPrimaryAction)
            } label: {
                Label("Import Library", systemImage: "shippingbox")
                    .frame(minWidth: 150)
            }
            .buttonStyle(.borderedProminent)

            Menu {
                Button {
                    presentFileImporter(.musicFiles)
                } label: {
                    Label("Import Music", systemImage: "square.and.arrow.down")
                }

                Button {
                    presentFileImporter(.musicFolder)
                } label: {
                    Label("Import Folder", systemImage: "folder.badge.plus")
                }
            } label: {
                Label("More Import Options", systemImage: "ellipsis.circle")
            }
            .buttonStyle(.bordered)
        }
        .padding(24)
        .frame(maxWidth: 360)
    }

    private var libraryActionsMenu: some View {
        Menu {
            Button {
                presentFileImporter(.musicFiles)
            } label: {
                Label("Import Files", systemImage: "music.note.list")
            }

            Button {
                presentFileImporter(.musicFolder)
            } label: {
                Label("Import Folder", systemImage: "folder.badge.plus")
            }

            Divider()

            Button {
                presentFileImporter(.libraryPackage)
            } label: {
                Label("Import Library", systemImage: "square.and.arrow.down")
            }

            Divider()

            Button {
                Task { await model.refreshLibrary() }
            } label: {
                Label("Refresh", systemImage: "arrow.clockwise")
            }

            sortMenu

            Divider()

            Button(role: .destructive) {
                isZeroOutConfirmationPresented = true
            } label: {
                Label("Zero Out Library", systemImage: "trash")
            }
        } label: {
            Label("Actions", systemImage: "ellipsis.circle")
        }
        .disabled(model.operations.isBusy)
    }

    private var sortMenu: some View {
        Menu {
            ForEach(PlaylistSortMode.allCases) { sortMode in
                Button {
                    Task { await model.sortVisibleTracks(sortMode) }
                } label: {
                    Label(
                        sortMode.label,
                        systemImage: model.playlists.sortMode == sortMode ? "checkmark" : sortMode.systemImage
                    )
                }
            }
        } label: {
            Label("Sort", systemImage: "arrow.up.arrow.down")
        }
    }

    private var miniPlayerBar: some View {
        Group {
            if let track = model.playback.nowPlaying {
                HStack(spacing: 12) {
                    Button {
                        isNowPlayingPresented = true
                    } label: {
                        HStack(spacing: 12) {
                            PhoneArtworkImage(
                                artworkURL: track.artworkURL,
                                placeholderSystemImage: "music.note",
                                size: 42,
                                cornerRadius: 7
                            )

                            VStack(alignment: .leading, spacing: 2) {
                                Text(track.phoneDisplayTitle)
                                    .font(.subheadline.weight(.semibold))
                                    .lineLimit(2)
                                    .layoutPriority(1)
                                Text(track.phoneDisplaySubtitle)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                                    .layoutPriority(1)
                            }

                            Spacer(minLength: 0)
                        }
                        .contentShape(Rectangle())
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint("Opens Now Playing")

                    Button {
                        Task { await model.pauseOrResume() }
                    } label: {
                        Image(systemName: model.playback.isPlaying ? "pause.fill" : "play.fill")
                            .font(.title3)
                    }

                    Button {
                        Task { await model.nextTrack() }
                    } label: {
                        Image(systemName: "forward.fill")
                            .font(.title3)
                    }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .background(.bar)
            }
        }
    }

    private func playerControls(compact: Bool) -> some View {
        VStack(spacing: compact ? 7 : 10) {
            Slider(
                value: seekBinding,
                in: 0...1,
                onEditingChanged: { editing in
                    if !editing, let progress = pendingSeekProgress {
                        pendingSeekProgress = nil
                        Task { await model.seek(toProgress: progress) }
                    }
                }
            )
            .disabled(model.playback.nowPlaying?.durationMS == nil)

            HStack {
                Text(model.playbackTimeText)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                Spacer()
            }

            HStack(spacing: 0) {
                PhonePlaybackModeMenu(model: model, showsTitle: false)
                .frame(width: 44, height: 44)

                Spacer(minLength: 0)
                Button {
                    Task { await model.previousTrack() }
                } label: {
                    Image(systemName: "backward.fill")
                }
                .frame(width: 44, height: 44)
                .accessibilityLabel("Previous Track")

                Spacer(minLength: 0)
                Button {
                    Task { await model.pauseOrResume() }
                } label: {
                    Image(systemName: model.playback.isPlaying ? "pause.circle.fill" : "play.circle.fill")
                        .font(.system(size: compact ? 46 : 50))
                }
                .frame(width: 52, height: 52)
                .accessibilityLabel(model.playback.isPlaying ? "Pause" : "Play")

                Spacer(minLength: 0)
                Button {
                    Task { await model.nextTrack() }
                } label: {
                    Image(systemName: "forward.fill")
                }
                .frame(width: 44, height: 44)
                .accessibilityLabel("Next Track")

                Spacer(minLength: 0)
                Button {
                    isQueuePresented = true
                } label: {
                    Image(systemName: "music.note.list")
                }
                .frame(width: 44, height: 44)
                .accessibilityLabel(model.queueStatusText)
                .accessibilityHint("Shows the global playing queue")
            }
            .font(.title2)
            .buttonStyle(.plain)
        }
    }

    private var busyOverlay: some View {
        ZStack {
            Color.black.opacity(0.18)
                .ignoresSafeArea()
            VStack(spacing: 12) {
                if let progress = model.operations.libraryProgress {
                    ProgressView(value: progress)
                        .progressViewStyle(.linear)
                        .frame(maxWidth: .infinity)
                } else {
                    ProgressView()
                        .controlSize(.large)
                }
                Text(model.operations.libraryStatus.isEmpty ? model.operations.status : model.operations.libraryStatus)
                    .font(.callout.weight(.medium))
                    .multilineTextAlignment(.center)
                    .lineLimit(5)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(20)
            .frame(maxWidth: 320)
            .padding(.horizontal, 24)
            .background(.regularMaterial)
            .clipShape(RoundedRectangle(cornerRadius: 12))
        }
    }

    private func loadingPlaceholder(_ title: String) -> some View {
        VStack(spacing: 14) {
            ProgressView()
                .controlSize(.large)
            Text(title)
                .font(.callout.weight(.medium))
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(title)
    }

    private func startupFailureView(message: String) -> some View {
        VStack(spacing: 14) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 42))
                .foregroundStyle(.orange)
            Text("Unable to Start")
                .font(.title2.weight(.semibold))
            Text(message)
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .textSelection(.enabled)
        }
        .padding(28)
        .frame(maxWidth: 420)
        .accessibilityElement(children: .combine)
    }

    private func presentError(_ error: String) {
        let message = error.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !message.isEmpty else {
            return
        }
        activeAlert = PhoneAppAlert(
            title: model.startupError == nil ? "NormalPlayer" : "Unable to Start",
            message: message
        )
    }

    private var seekBinding: Binding<Double> {
        Binding(
            get: { pendingSeekProgress ?? model.playbackProgress ?? 0 },
            set: { pendingSeekProgress = $0 }
        )
    }

    private var emptyIcon: String {
        switch model.library.scope {
        case .library:
            return "music.note.list"
        case .favorites:
            return "heart"
        case .history:
            return "clock"
        case .playlist:
            return "music.note.house"
        }
    }

    @ViewBuilder
    private func trackContextMenu(for track: TrackItem) -> some View {
        Button {
            play(track)
        } label: {
            Label("Play", systemImage: "play.fill")
        }

        Button {
            Task { await model.playNext(track) }
        } label: {
            Label("Play Next", systemImage: "text.line.first.and.arrowtriangle.forward")
        }

        Button {
            Task { await model.addToQueue(track) }
        } label: {
            Label("Add to Queue", systemImage: "text.badge.plus")
        }

        Button {
            presentPlaylistPicker(for: track)
        } label: {
            Label("Add to Playlist", systemImage: "music.note.list")
        }

        Divider()

        Button(role: .destructive) {
            presentLibraryDeletion(for: track)
        } label: {
            Label("Delete from Library…", systemImage: "trash")
        }
    }

    private func play(_ track: TrackItem) {
        model.selectTrack(id: track.id)
        Task { await model.play(track) }
    }

    private func materialize(_ track: TrackItem) {
        model.selectTrack(id: track.id)
        Task {
            let destination = exportDestination(for: track)
            await model.materializeSelected(to: destination)
            lastExportURL = destination
        }
    }

    private func presentPlaylistPicker(for track: TrackItem) {
        model.selectTrack(id: track.id)
        model.presentPlaylistPicker(for: track)
        Task { await model.refreshPlaylists() }
    }

    private func presentLibraryDeletion(for track: TrackItem) {
        pendingLibraryDeletion = track
        isLibraryDeletionConfirmationPresented = true
    }

    private func exportDestination(for track: TrackItem) -> URL {
        let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let exportDirectory = documents
            .appendingPathComponent("NormalPlayer", isDirectory: true)
            .appendingPathComponent("Exports", isDirectory: true)
        try? FileManager.default.createDirectory(at: exportDirectory, withIntermediateDirectories: true)
        return exportDirectory.appendingPathComponent(defaultExportFileName(for: track))
    }

    private func defaultExportFileName(for track: TrackItem) -> String {
        let title = sanitizedFileComponent(track.title)
        let fileExtension = track.formatName?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            ?? URL(fileURLWithPath: track.path).pathExtension.lowercased()
        return fileExtension.isEmpty ? title : "\(title).\(fileExtension)"
    }

    private func sanitizedFileComponent(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        let fallback = trimmed.isEmpty ? "NormalPlayer Export" : trimmed
        return fallback.replacingOccurrences(
            of: #"[/:]"#,
            with: "-",
            options: .regularExpression
        )
    }

    private func presentFileImporter(_ purpose: PhoneFileImportPurpose) {
        fileImportPurpose = purpose
        model.operations.status = purpose.presentationStatus
        model.playback.detail = ""
        DispatchQueue.main.async {
            isFileImporterPresented = true
        }
    }

    private func handleFileImport(_ result: Result<[URL], Error>) {
        isFileImporterPresented = false
        guard let purpose = fileImportPurpose else {
            model.operations.status = "Import selection was cancelled"
            model.playback.detail = "File picker returned without an import purpose"
            return
        }
        fileImportPurpose = nil

        do {
            let urls = try result.get()
            guard !urls.isEmpty else {
                model.operations.status = "No files selected"
                model.playback.detail = "File picker returned an empty selection"
                return
            }
            model.operations.status = "Selected \(urls.count) item\(urls.count == 1 ? "" : "s")"
            model.playback.detail = urls.map(\.lastPathComponent).joined(separator: ", ")
            Task {
                await handleImportedURLs(urls, purpose: purpose)
            }
        } catch {
            model.operations.status = error.localizedDescription
            model.playback.error = error.localizedDescription
        }
    }

    @MainActor
    private func handleImportedURLs(_ urls: [URL], purpose: PhoneFileImportPurpose) async {
        switch purpose {
        case .musicFiles:
            await model.importFiles(urls)
        case .musicFolder:
            guard let folder = urls.first else {
                return
            }
            await model.importFolder(folder)
        case .libraryPackage:
            guard let packageURL = urls.first else {
                return
            }
            await model.importLibrary(from: packageURL)
        case .trackCover(let track):
            guard let url = urls.first else {
                return
            }
            await model.setTrackArtwork(for: track, imageURL: url)
        case .albumCover(let track):
            guard let url = urls.first else {
                return
            }
            await model.setAlbumArtwork(for: track, imageURL: url)
        case .playlistCover(let playlist):
            guard let url = urls.first else {
                return
            }
            await model.setPlaylistArtwork(playlist, imageURL: url)
        case .playlistSettingsArtwork:
            guard let url = urls.first else {
                return
            }
            model.setPlaylistSettingsArtworkURL(url)
        case .editArtwork:
            guard let url = urls.first else {
                return
            }
            model.setTrackEditArtworkURL(url)
        case .editLyrics:
            guard let url = urls.first else {
                return
            }
            model.setTrackEditLyricsURL(url)
        }
    }

    @MainActor
    private func prepareLibraryExport() async {
        let packageURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("SilentLibraryExports", isDirectory: true)
            .appendingPathComponent(
                "Silent-Library-\(UUID().uuidString).silentlibrary",
                isDirectory: true
            )
        guard await model.exportLibrary(to: packageURL) != nil else {
            return
        }
        pendingLibraryExportURL = packageURL
        model.operations.status = "Choose where to save the library package"
        isLibraryExporterPresented = true
    }

    private func handleLibraryExport(_ result: Result<[URL], Error>) {
        isLibraryExporterPresented = false
        let exportedPackage = pendingLibraryExportURL
        pendingLibraryExportURL = nil
        defer {
            if let exportedPackage {
                try? FileManager.default.removeItem(at: exportedPackage)
            }
        }

        do {
            let destinations = try result.get()
            guard let destination = destinations.first else {
                model.operations.status = "Library export cancelled"
                return
            }
            model.operations.status = "Library exported"
            model.playback.detail = destination.path
        } catch {
            model.operations.status = "Library export failed"
            model.playback.error = error.localizedDescription
        }
    }
}

#endif
