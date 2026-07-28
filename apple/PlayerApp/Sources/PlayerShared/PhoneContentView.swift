#if os(iOS)
import Foundation
import SwiftUI
import UIKit
import UniformTypeIdentifiers

private extension UTType {
    static let silentLibraryPackage = UTType(
        exportedAs: "com.normalplayer.silent-library",
        conformingTo: .package
    )
}

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
    @State private var isQueuePresented = false

    public init(model: AppModel) {
        self.model = model
    }

    public var body: some View {
        ZStack {
            if let startupError = model.startupError {
                startupFailureView(message: startupError)
            } else if isRestoringPresentation {
                restorationPlaceholder
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

                    nowPlayingTab
                        .tabItem {
                            Label("Player", systemImage: "play.circle")
                        }
                        .tag(PhonePresentationTab.nowPlaying)
                }
            }

            if model.isBusy {
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
        .sheet(isPresented: $isQueuePresented) {
            PhonePlaybackQueueSheet(model: model)
        }
        .sheet(isPresented: $model.isPlaylistCreatePresented) {
            PhonePlaylistCreateSheet(model: model)
        }
        .sheet(isPresented: $model.isPlaylistPickerPresented) {
            PhonePlaylistPickerSheet(model: model)
        }
        .sheet(isPresented: $model.isPlaylistSettingsPresented) {
            PhonePlaylistSettingsSheet(model: model) {
                presentFileImporter(.playlistSettingsArtwork)
            }
        }
        .sheet(isPresented: $model.isViewEditPresented) {
            PhoneTrackEditSheet(
                model: model,
                chooseArtwork: { presentFileImporter(.editArtwork) },
                chooseLyrics: { presentFileImporter(.editLyrics) }
            )
        }
        .task {
            await restorePresentation()
        }
        .onChange(of: model.playbackError) { error in
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
            case .nowPlaying:
                persistPresentation()
            }
        }
        .onChange(of: playlistPath) { _ in
            persistPresentation()
        }
        .onChange(of: model.libraryScope) { _ in
            persistPresentation()
        }
        .onChange(of: model.selectedTrack?.id) { _ in
            persistPresentation()
        }
        .onChange(of: model.playlists) { _ in
            repairPlaylistPath()
            persistPresentation()
        }
        .onChange(of: scenePhase) { phase in
            if phase == .background {
                persistPresentation()
            }
        }
        .alert(item: $activeAlert) { alert in
            Alert(
                title: Text(alert.title),
                message: Text(alert.message),
                dismissButton: .default(Text("OK")) {
                    model.playbackError = ""
                }
            )
        }
    }

    private var libraryTab: some View {
        NavigationStack {
            trackList(scopeTitle: model.libraryScope.title)
                .navigationTitle("Library")
                .searchable(text: $model.query, prompt: "Title, artist, album")
                .onSubmit(of: .search) {
                    Task { await model.search() }
                }
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
                            Label(model.libraryScope.title, systemImage: "line.3.horizontal")
                        }
                    }

                    ToolbarItemGroup(placement: .topBarTrailing) {
                        Button {
                            Task { await model.playEntireLibrary() }
                        } label: {
                            Label("Play All \(model.libraryScope.title)", systemImage: "play.fill")
                        }
                        .disabled(model.tracks.isEmpty || model.isBusy)

                        libraryActionsMenu
                    }
                }
                .safeAreaInset(edge: .bottom) {
                    miniPlayerBar
                }
        }
    }

    private var searchTab: some View {
        NavigationStack {
            trackList(scopeTitle: "Search")
                .navigationTitle("Search")
                .searchable(text: $model.query, prompt: "Search music")
                .onSubmit(of: .search) {
                    Task { await model.search() }
                }
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button {
                            model.query = ""
                            Task { await model.reloadActiveScope() }
                        } label: {
                            Label("Clear", systemImage: "xmark.circle")
                        }
                    }
                }
                .safeAreaInset(edge: .bottom) {
                    miniPlayerBar
                }
        }
    }

    private var playlistsTab: some View {
        NavigationStack(path: $playlistPath) {
            List {
                if !model.recentPlaylists.isEmpty {
                    Section("Recent") {
                        ForEach(model.recentPlaylists) { playlist in
                            phonePlaylistLink(playlist)
                        }
                    }
                }

                Section(model.recentPlaylists.isEmpty ? "Playlists" : "All Playlists") {
                    ForEach(model.playlists) { playlist in
                        phonePlaylistLink(playlist)
                    }
                }
            }
            .overlay {
                if model.playlists.isEmpty {
                    PhoneEmptyState(
                        title: "No Playlists",
                        message: model.status,
                        systemImage: "music.note.house"
                    )
                }
            }
            .navigationTitle("Playlists")
            .toolbar {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    Button {
                        Task { await model.refreshPlaylists() }
                    } label: {
                        Label("Refresh", systemImage: "arrow.clockwise")
                    }

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
            .safeAreaInset(edge: .bottom) {
                miniPlayerBar
            }
            .navigationDestination(for: PhoneRoute.self) { route in
                switch route {
                case .playlist(let id):
                    if let playlist = model.playlists.first(where: { $0.id == id }) {
                        PhonePlaylistDetailView(model: model, playlist: playlist)
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
                    Text("\(playlist.trackCount) tracks")
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
        let requestedSnapshot = PhonePresentationPersistence.decode(sceneSession)
            ?? PhonePresentationPersistence.load()
            ?? .initial

        selectedTab = requestedSnapshot.selectedTab
        await model.bootstrap(
            restoring: requestedSnapshot.bootstrapScope,
            preferredSelectedViewID: requestedSnapshot.selectedViewID
        )

        let validatedSnapshot = requestedSnapshot.validated(against: model.playlists)
        if validatedSnapshot.selectedTab == .playlists,
           let playlistID = validatedSnapshot.playlistDetailID {
            playlistPath = [.playlist(playlistID)]
        } else {
            playlistPath = []
        }

        isRestoringPresentation = false
        persistPresentation()
        presentError(model.playbackError)
    }

    private func persistPresentation() {
        guard !isRestoringPresentation else {
            return
        }

        let snapshot = PhonePresentationSnapshot(
            selectedTab: selectedTab,
            contentScope: presentationScope,
            playlistDetailID: playlistPath.last?.playlistID,
            selectedViewID: model.selectedTrack?.id
        )
        guard let encoded = PhonePresentationPersistence.encode(snapshot) else {
            return
        }
        sceneSession = encoded
        PhonePresentationPersistence.save(snapshot)
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
              !model.playlists.contains(where: { $0.id == route.playlistID }) else {
            return
        }
        playlistPath = []
    }

    private var nowPlayingTab: some View {
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
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        isQueuePresented = true
                    } label: {
                        Label("Queue", systemImage: "music.note.list")
                    }
                }
            }
        }
    }

    private func nowPlayingContent(
        artworkSize: CGFloat,
        spacing: CGFloat,
        usesCompactControls: Bool
    ) -> some View {
        VStack(spacing: spacing) {
            let nowDetails = details(for: model.nowPlaying)
            PhoneArtworkImage(
                artworkURL: nowDetails?.artworkURL ?? model.nowPlaying?.artworkURL,
                placeholderSystemImage: "music.note",
                size: artworkSize,
                cornerRadius: 12
            )

            VStack(spacing: usesCompactControls ? 3 : 5) {
                Text(model.nowPlaying?.phoneDisplayTitle ?? "Nothing Playing")
                    .font(
                        usesCompactControls
                            ? .headline
                            : .title3.weight(.semibold)
                    )
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .layoutPriority(1)
                Text(model.nowPlaying?.phoneDisplaySubtitle ?? model.status.phoneCompacted)
                    .font(usesCompactControls ? .footnote : .callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }

            playerControls(compact: usesCompactControls)
        }
        .frame(maxWidth: .infinity)
    }

    private func details(for track: TrackItem?) -> TrackDetails? {
        guard let track,
              let details = model.nowPlayingDetails,
              details.viewID == track.viewID else {
            return nil
        }
        return details
    }

    private func trackList(scopeTitle: String) -> some View {
        List {
            ForEach(model.tracks) { track in
                Button {
                    play(track)
                } label: {
                    PhoneTrackRow(
                        track: track,
                        isCurrent: model.nowPlaying?.id == track.id,
                        isPlaying: model.nowPlaying?.id == track.id && model.isPlaying
                    )
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Play \(track.phoneDisplayTitle)")
                .accessibilityHint("Starts this track and queues the visible songs")
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
            if model.tracks.isEmpty {
                PhoneEmptyState(
                    title: scopeTitle,
                    message: model.status,
                    systemImage: emptyIcon
                )
            }
        }
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
        .disabled(model.isBusy)
    }

    private var sortMenu: some View {
        Menu {
            ForEach(PlaylistSortMode.allCases) { sortMode in
                Button {
                    Task { await model.sortVisibleTracks(sortMode) }
                } label: {
                    Label(
                        sortMode.label,
                        systemImage: model.playlistSortMode == sortMode ? "checkmark" : sortMode.systemImage
                    )
                }
            }
        } label: {
            Label("Sort", systemImage: "arrow.up.arrow.down")
        }
    }

    private var miniPlayerBar: some View {
        Group {
            if let track = model.nowPlaying {
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

                    Spacer()

                    Button {
                        Task { await model.pauseOrResume() }
                    } label: {
                        Image(systemName: model.isPlaying ? "pause.fill" : "play.fill")
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
                .contentShape(Rectangle())
                .onTapGesture {
                    selectedTab = .nowPlaying
                }
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
            .disabled(model.nowPlaying?.durationMS == nil)

            HStack {
                Text(model.playbackTimeText)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                Spacer()
            }

            HStack(spacing: 0) {
                Button {
                    Task { await model.toggleShuffle() }
                } label: {
                    Image(systemName: "shuffle")
                        .foregroundStyle(model.isShuffleEnabled ? Color.accentColor : Color.secondary)
                }
                .frame(width: 44, height: 44)
                .accessibilityLabel("Shuffle")
                .accessibilityValue(model.isShuffleEnabled ? "On" : "Off")

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
                    Image(systemName: model.isPlaying ? "pause.circle.fill" : "play.circle.fill")
                        .font(.system(size: compact ? 46 : 50))
                }
                .frame(width: 52, height: 52)
                .accessibilityLabel(model.isPlaying ? "Pause" : "Play")

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
                    Task { await model.cycleRepeatMode() }
                } label: {
                    Image(systemName: model.repeatMode.systemImage)
                        .foregroundStyle(model.repeatMode == .off ? Color.secondary : Color.accentColor)
                }
                .frame(width: 44, height: 44)
                .accessibilityLabel("Repeat")
                .accessibilityValue(model.repeatMode.label)
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
                if let progress = model.libraryProgress {
                    ProgressView(value: progress)
                        .progressViewStyle(.linear)
                        .frame(maxWidth: .infinity)
                } else {
                    ProgressView()
                        .controlSize(.large)
                }
                Text(model.libraryStatus.isEmpty ? model.status : model.libraryStatus)
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

    private var restorationPlaceholder: some View {
        VStack(spacing: 14) {
            ProgressView()
                .controlSize(.large)
            Text("Restoring Player")
                .font(.callout.weight(.medium))
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Restoring player")
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
        switch model.libraryScope {
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
        model.status = purpose.presentationStatus
        model.playbackDetail = ""
        DispatchQueue.main.async {
            isFileImporterPresented = true
        }
    }

    private func handleFileImport(_ result: Result<[URL], Error>) {
        isFileImporterPresented = false
        guard let purpose = fileImportPurpose else {
            model.status = "Import selection was cancelled"
            model.playbackDetail = "File picker returned without an import purpose"
            return
        }
        fileImportPurpose = nil

        do {
            let urls = try result.get()
            guard !urls.isEmpty else {
                model.status = "No files selected"
                model.playbackDetail = "File picker returned an empty selection"
                return
            }
            model.status = "Selected \(urls.count) item\(urls.count == 1 ? "" : "s")"
            model.playbackDetail = urls.map(\.lastPathComponent).joined(separator: ", ")
            Task {
                await handleImportedURLs(urls, purpose: purpose)
            }
        } catch {
            model.status = error.localizedDescription
            model.playbackError = error.localizedDescription
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
            model.setViewEditArtworkURL(url)
        case .editLyrics:
            guard let url = urls.first else {
                return
            }
            model.setViewEditLyricsURL(url)
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
        model.status = "Choose where to save the library package"
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
                model.status = "Library export cancelled"
                return
            }
            model.status = "Library exported"
            model.playbackDetail = destination.path
        } catch {
            model.status = "Library export failed"
            model.playbackError = error.localizedDescription
        }
    }
}

private struct PhonePlaybackQueueSheet: View {
    @ObservedObject var model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                if model.playbackQueue.isEmpty {
                    VStack(spacing: 10) {
                        Image(systemName: "music.note.list")
                            .font(.system(size: 36))
                            .foregroundStyle(.secondary)
                        Text("Queue Is Empty")
                            .font(.headline)
                        Text("Use Play Next or Add to Queue from a song.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity, minHeight: 220)
                    .listRowSeparator(.hidden)
                } else {
                    ForEach(Array(model.playbackQueue.enumerated()), id: \.element.id) { index, track in
                        HStack(spacing: 12) {
                            Image(systemName: model.queuePosition == index ? "speaker.wave.2.fill" : "line.3.horizontal")
                                .foregroundStyle(model.queuePosition == index ? Color.accentColor : Color.secondary)
                                .frame(width: 20)

                            VStack(alignment: .leading, spacing: 2) {
                                Text(track.phoneDisplayTitle)
                                    .lineLimit(2)
                                Text(track.phoneDisplaySubtitle)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                        }
                        .accessibilityElement(children: .combine)
                        .accessibilityValue(model.queuePosition == index ? "Currently selected" : "")
                    }
                    .onMove(perform: move)
                    .onDelete { offsets in
                        Task {
                            for index in offsets.sorted(by: >) {
                                await model.removeQueueItem(at: index)
                            }
                        }
                    }
                }
            }
            .navigationTitle("Playing Queue")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }

                ToolbarItemGroup(placement: .topBarTrailing) {
                    EditButton()
                        .disabled(model.playbackQueue.isEmpty)

                    Button("Clear", role: .destructive) {
                        Task { await model.clearPlaybackQueue() }
                    }
                    .disabled(model.playbackQueue.isEmpty)
                }
            }
        }
        .task {
            await model.refreshPlaybackState()
        }
    }

    private func move(from offsets: IndexSet, to destination: Int) {
        guard let source = offsets.first else {
            return
        }
        let target = destination > source ? destination - 1 : destination
        guard model.playbackQueue.indices.contains(target) else {
            return
        }
        Task { await model.moveQueueItem(from: source, to: target) }
    }
}

private struct PhoneAppAlert: Identifiable {
    let id = UUID()
    let title: String
    let message: String
}

private struct PhoneDocumentPickerBridge: UIViewControllerRepresentable {
    @Binding var isPresented: Bool
    let purpose: PhoneFileImportPurpose?
    let onResult: (Result<[URL], Error>) -> Void

    func makeUIViewController(context: Context) -> UIViewController {
        UIViewController()
    }

    func updateUIViewController(_ viewController: UIViewController, context: Context) {
        context.coordinator.parent = self

        guard isPresented, let purpose else {
            if context.coordinator.presentedPicker != nil {
                context.coordinator.dismissPresentedPicker()
            }
            return
        }

        guard context.coordinator.presentedPicker == nil else {
            return
        }

        DispatchQueue.main.async {
            guard isPresented, context.coordinator.presentedPicker == nil else {
                return
            }
            let picker = UIDocumentPickerViewController(
                forOpeningContentTypes: purpose.allowedContentTypes,
                asCopy: purpose.importsAsCopy
            )
            picker.delegate = context.coordinator
            picker.allowsMultipleSelection = purpose.allowsMultipleSelection
            picker.shouldShowFileExtensions = true
            context.coordinator.presentedPicker = picker
            context.coordinator.topPresenter(from: viewController).present(picker, animated: true)
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    final class Coordinator: NSObject, UIDocumentPickerDelegate {
        var parent: PhoneDocumentPickerBridge
        weak var presentedPicker: UIDocumentPickerViewController?

        init(parent: PhoneDocumentPickerBridge) {
            self.parent = parent
        }

        func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
            presentedPicker = nil
            parent.isPresented = false
            parent.onResult(.success(urls))
        }

        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
            presentedPicker = nil
            parent.isPresented = false
            parent.onResult(.success([]))
        }

        func dismissPresentedPicker() {
            presentedPicker?.dismiss(animated: true)
            presentedPicker = nil
        }

        func topPresenter(from viewController: UIViewController) -> UIViewController {
            var presenter = viewController.view.window?.rootViewController ?? viewController
            while let presented = presenter.presentedViewController {
                presenter = presented
            }
            return presenter
        }
    }
}

private struct PhoneDocumentExporterBridge: UIViewControllerRepresentable {
    @Binding var isPresented: Bool
    let sourceURL: URL?
    let onResult: (Result<[URL], Error>) -> Void

    func makeUIViewController(context: Context) -> UIViewController {
        UIViewController()
    }

    func updateUIViewController(_ viewController: UIViewController, context: Context) {
        context.coordinator.parent = self

        guard isPresented, let sourceURL else {
            if context.coordinator.presentedPicker != nil {
                context.coordinator.dismissPresentedPicker()
            }
            return
        }

        guard context.coordinator.presentedPicker == nil else {
            return
        }

        DispatchQueue.main.async {
            guard isPresented, context.coordinator.presentedPicker == nil else {
                return
            }
            let picker = UIDocumentPickerViewController(
                forExporting: [sourceURL],
                asCopy: true
            )
            picker.delegate = context.coordinator
            picker.shouldShowFileExtensions = true
            context.coordinator.presentedPicker = picker
            context.coordinator.topPresenter(from: viewController).present(picker, animated: true)
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    final class Coordinator: NSObject, UIDocumentPickerDelegate {
        var parent: PhoneDocumentExporterBridge
        weak var presentedPicker: UIDocumentPickerViewController?

        init(parent: PhoneDocumentExporterBridge) {
            self.parent = parent
        }

        func documentPicker(
            _ controller: UIDocumentPickerViewController,
            didPickDocumentsAt urls: [URL]
        ) {
            presentedPicker = nil
            parent.isPresented = false
            parent.onResult(.success(urls))
        }

        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
            presentedPicker = nil
            parent.isPresented = false
            parent.onResult(.success([]))
        }

        func dismissPresentedPicker() {
            presentedPicker?.dismiss(animated: true)
            presentedPicker = nil
        }

        func topPresenter(from viewController: UIViewController) -> UIViewController {
            var presenter = viewController.view.window?.rootViewController ?? viewController
            while let presented = presenter.presentedViewController {
                presenter = presented
            }
            return presenter
        }
    }
}

private enum PhoneFileImportPurpose {
    case musicFiles
    case musicFolder
    case libraryPackage
    case trackCover(TrackItem)
    case albumCover(TrackItem)
    case playlistCover(PlaylistItem)
    case playlistSettingsArtwork
    case editArtwork
    case editLyrics

    var allowedContentTypes: [UTType] {
        switch self {
        case .musicFiles:
            // OGG/FLAC can arrive as dynamic UTTypes on iOS, so Rust owns the final audio filter.
            return [.item]
        case .musicFolder:
            return [.folder]
        case .libraryPackage:
            return [.silentLibraryPackage, .package]
        case .trackCover, .albumCover, .playlistCover, .playlistSettingsArtwork, .editArtwork:
            return [.image]
        case .editLyrics:
            return [
                UTType(filenameExtension: "lrc") ?? .plainText,
                UTType(filenameExtension: "lyrics") ?? .plainText,
                .plainText
            ]
        }
    }

    var importsAsCopy: Bool {
        switch self {
        case .musicFolder:
            return false
        case .musicFiles, .libraryPackage, .trackCover, .albumCover, .playlistCover, .playlistSettingsArtwork, .editArtwork, .editLyrics:
            return true
        }
    }

    var allowsMultipleSelection: Bool {
        switch self {
        case .musicFiles:
            return true
        case .musicFolder, .libraryPackage, .trackCover, .albumCover, .playlistCover, .playlistSettingsArtwork, .editArtwork, .editLyrics:
            return false
        }
    }

    var presentationStatus: String {
        switch self {
        case .musicFiles:
            return "Choose music files"
        case .musicFolder:
            return "Choose a music folder"
        case .libraryPackage:
            return "Choose a Silent library package"
        case .trackCover:
            return "Choose track artwork"
        case .albumCover:
            return "Choose album artwork"
        case .playlistCover, .playlistSettingsArtwork:
            return "Choose playlist artwork"
        case .editArtwork:
            return "Choose view artwork"
        case .editLyrics:
            return "Choose lyrics file"
        }
    }
}

private struct PhoneTrackDetailView: View {
    @ObservedObject var model: AppModel
    let track: TrackItem
    let requestAddToPlaylist: (TrackItem) -> Void
    let requestTrackCover: (TrackItem) -> Void
    let requestAlbumCover: (TrackItem) -> Void
    let exportView: (TrackItem) -> Void

    var body: some View {
        let currentTrack = displayedTrack
        let currentDetails = details

        List {
            Section {
                PhoneTrackDetailHeader(
                    track: currentTrack,
                    details: currentDetails,
                    isPlaying: model.nowPlaying?.id == currentTrack.id && model.isPlaying
                )
                .frame(maxWidth: .infinity)
                .listRowInsets(EdgeInsets(top: 20, leading: 16, bottom: 20, trailing: 16))
            }

            Section("Playback") {
                Button {
                    model.selectTrack(id: currentTrack.id)
                    Task { await model.play(currentTrack) }
                } label: {
                    Label("Play", systemImage: "play.fill")
                }

                LabeledContent("Position", value: model.nowPlaying?.id == currentTrack.id ? model.playbackTimeText : currentTrack.durationText)
                LabeledContent("Loudness", value: currentTrack.gainText)
                LabeledContent("Queue", value: model.nowPlaying?.id == currentTrack.id ? model.queueStatusText : "Not queued")
            }

            Section("View") {
                if viewChoices.count > 1 {
                    Picker("Active View", selection: viewBinding) {
                        ForEach(viewChoices) { choice in
                            Text(viewChoiceTitle(choice))
                                .tag(choice.id)
                        }
                    }
                } else {
                    LabeledContent("Active View", value: viewTitle(for: currentTrack, index: 0))
                }

                Picker("Rating", selection: ratingBinding) {
                    Text("Unrated").tag(0)
                    ForEach(1...10, id: \.self) { value in
                        Text("\(value)/10").tag(value)
                    }
                }

                if let currentDetails {
                    LabeledContent("Kind", value: currentDetails.isPrimaryView ? "Primary" : "Derived")
                    LabeledContent("Format", value: optionalValue(currentDetails.formatName ?? currentTrack.formatName))
                    LabeledContent("Quality", value: optionalValue(currentDetails.qualityProfile ?? currentTrack.qualityProfile))
                }
            }

            Section("Metadata") {
                LabeledContent("Title", value: currentDetails?.displayTitle ?? currentTrack.title)
                LabeledContent("Artist", value: currentDetails?.displayArtist ?? currentTrack.artist)
                LabeledContent("Album", value: currentDetails?.displayAlbum ?? currentTrack.album)

                if let currentDetails, hasOriginalMetadata(currentDetails) {
                    DisclosureGroup("Original Metadata") {
                        LabeledContent("Title", value: currentDetails.originalTitle)
                        LabeledContent("Artist", value: currentDetails.originalArtist)
                        LabeledContent("Album", value: currentDetails.originalAlbum)
                    }
                }
            }

            if let lyrics = currentDetails?.lyricsText?.trimmingCharacters(in: .whitespacesAndNewlines),
               !lyrics.isEmpty {
                Section("Lyrics") {
                    Text(lyrics)
                        .font(.body)
                        .textSelection(.enabled)
                }
            }

            if let notes = currentDetails?.notes?.trimmingCharacters(in: .whitespacesAndNewlines),
               !notes.isEmpty {
                Section("Notes") {
                    Text(notes)
                        .font(.body)
                        .textSelection(.enabled)
                }
            }

            if let currentDetails {
                let importantDiagnostics = currentDetails.diagnostics.filter { $0.severity != .info }
                if !importantDiagnostics.isEmpty {
                    Section("Needs Attention") {
                        ForEach(importantDiagnostics) { diagnostic in
                            PhoneDiagnosticRow(diagnostic: diagnostic)
                        }
                    }
                }
            }

            Section("Actions") {
                Button {
                    Task { await model.playNext(currentTrack) }
                } label: {
                    Label("Play Next", systemImage: "text.line.first.and.arrowtriangle.forward")
                }

                Button {
                    Task { await model.addToQueue(currentTrack) }
                } label: {
                    Label("Add to Queue", systemImage: "text.badge.plus")
                }

                Button {
                    requestAddToPlaylist(currentTrack)
                } label: {
                    Label("Add to Playlist", systemImage: "text.badge.plus")
                }

                Button {
                    model.selectTrack(id: currentTrack.id)
                    model.presentViewEdit()
                } label: {
                    Label("Edit Current View", systemImage: "pencil")
                }

                Button {
                    requestTrackCover(currentTrack)
                } label: {
                    Label("Set Track Cover", systemImage: "photo")
                }

                Button {
                    requestAlbumCover(currentTrack)
                } label: {
                    Label("Set Album Cover", systemImage: "rectangle.stack.badge.plus")
                }
                .disabled(!currentTrack.hasAlbumIdentity)

                Button {
                    exportView(currentTrack)
                } label: {
                    Label("Export Current View", systemImage: "square.and.arrow.up")
                }
            }
        }
        .navigationTitle(currentTrack.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItemGroup(placement: .bottomBar) {
                Button {
                    Task { await model.previousTrack() }
                } label: {
                    Label("Previous", systemImage: "backward.fill")
                }
                .disabled(model.nowPlaying == nil)

                Spacer()

                Button {
                    model.selectTrack(id: currentTrack.id)
                    Task { await model.play(currentTrack) }
                } label: {
                    Label("Play", systemImage: "play.fill")
                }

                Spacer()

                Button {
                    Task { await model.nextTrack() }
                } label: {
                    Label("Next", systemImage: "forward.fill")
                }
                .disabled(model.nowPlaying == nil)
            }
        }
        .task {
            model.selectTrack(id: track.id)
        }
    }

    private var displayedTrack: TrackItem {
        if let detailTrack = model.detailTrack,
           detailTrack.primaryViewID == track.primaryViewID {
            return detailTrack
        }
        return track
    }

    private var details: TrackDetails? {
        guard let details = model.nowPlayingDetails,
              details.viewID == displayedTrack.viewID else {
            return nil
        }
        return details
    }

    private var viewChoices: [TrackViewChoice] {
        guard model.detailTrack?.primaryViewID == displayedTrack.primaryViewID else {
            return []
        }
        return model.detailViewChoices
    }

    private var ratingBinding: Binding<Int> {
        Binding(
            get: { details?.rating ?? displayedTrack.rating ?? 0 },
            set: { value in
                model.selectTrack(id: displayedTrack.id)
                Task { await model.setRating(value == 0 ? nil : value) }
            }
        )
    }

    private var viewBinding: Binding<String> {
        Binding(
            get: { displayedTrack.id },
            set: { model.selectDetailView(id: $0) }
        )
    }

    private func viewChoiceTitle(_ choice: TrackViewChoice) -> String {
        viewTitle(for: choice.track, index: choice.index)
    }

    private func viewTitle(for track: TrackItem, index: Int) -> String {
        var title = track.viewName ?? (track.isPrimaryView ? "Primary View" : "View \(index + 1)")
        var details: [String] = []
        if let format = track.formatName?.trimmingCharacters(in: .whitespacesAndNewlines),
           !format.isEmpty {
            details.append(format.uppercased())
        }
        if let quality = track.qualityProfile?.trimmingCharacters(in: .whitespacesAndNewlines),
           !quality.isEmpty {
            details.append(quality)
        }
        if !details.isEmpty {
            title += " - " + details.joined(separator: " / ")
        }
        return title
    }

    private func hasOriginalMetadata(_ details: TrackDetails) -> Bool {
        details.originalTitle != details.displayTitle
            || details.originalArtist != details.displayArtist
            || details.originalAlbum != details.displayAlbum
    }

    private func optionalValue(_ value: String?) -> String {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return "Not set"
        }
        return value
    }
}

private struct PhoneTrackDetailHeader: View {
    let track: TrackItem
    let details: TrackDetails?
    let isPlaying: Bool

    var body: some View {
        VStack(spacing: 12) {
            PhoneArtworkImage(
                artworkURL: details?.artworkURL ?? track.artworkURL,
                placeholderSystemImage: isPlaying ? "speaker.wave.2.fill" : "music.note",
                size: 220,
                cornerRadius: 14
            )

            VStack(spacing: 4) {
                Text(details?.displayTitle ?? track.title)
                    .font(.title2.weight(.semibold))
                    .multilineTextAlignment(.center)
                    .lineLimit(3)
                Text(track.subtitle)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
                Text("\(track.durationText) · \(track.ratingText)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private struct PhoneDiagnosticRow: View {
    let diagnostic: TrackViewDiagnostic

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 3) {
                Text(diagnostic.title)
                    .font(.body)
                Text(diagnostic.detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        } icon: {
            Image(systemName: systemImage)
                .foregroundStyle(color)
        }
    }

    private var systemImage: String {
        switch diagnostic.severity {
        case .error:
            return "xmark.octagon.fill"
        case .warning:
            return "exclamationmark.triangle.fill"
        case .info:
            return "info.circle"
        }
    }

    private var color: Color {
        switch diagnostic.severity {
        case .error:
            return .red
        case .warning:
            return .orange
        case .info:
            return .secondary
        }
    }
}

private struct PhonePlaylistDetailView: View {
    @ObservedObject var model: AppModel
    let playlist: PlaylistItem

    var body: some View {
        List {
            Section {
                VStack(spacing: 16) {
                    PhoneArtworkImage(
                        artworkURL: playlist.artworkURL,
                        placeholderSystemImage: "music.note.house",
                        size: 112,
                        cornerRadius: 14
                    )

                    VStack(spacing: 4) {
                        Text(playlist.name.phoneCompacted)
                            .font(.title3.weight(.semibold))
                            .multilineTextAlignment(.center)
                            .fixedSize(horizontal: false, vertical: true)
                        Text("\(playlist.trackCount) tracks")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }

                    HStack(spacing: 12) {
                        Button {
                            Task {
                                await model.playPlaylist(playlist, shuffled: false)
                            }
                        } label: {
                            Label("Play", systemImage: "play.fill")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)

                        Button {
                            Task {
                                await model.playPlaylist(playlist, shuffled: true)
                            }
                        } label: {
                            Label("Shuffle", systemImage: "shuffle")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.bordered)
                    }
                    .controlSize(.large)
                    .disabled(playlist.trackCount == 0 || model.isBusy)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 8)
            }

            Section("Tracks") {
                ForEach(model.tracks) { track in
                    Button {
                        model.selectTrack(id: track.id)
                        Task {
                            await model.playPlaylist(
                                playlist,
                                startingAt: track,
                                shuffled: false
                            )
                        }
                    } label: {
                        PhoneTrackRow(
                            track: track,
                            isCurrent: model.nowPlaying?.id == track.id,
                            isPlaying: model.nowPlaying?.id == track.id && model.isPlaying
                        )
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Play \(track.phoneDisplayTitle)")
                    .accessibilityHint("Starts this track and queues the playlist")
                    .contextMenu {
                        Button {
                            Task {
                                await model.playPlaylist(
                                    playlist,
                                    startingAt: track,
                                    shuffled: false
                                )
                            }
                        } label: {
                            Label("Play from Here", systemImage: "play.fill")
                        }

                        Button {
                            Task {
                                await model.playPlaylist(
                                    playlist,
                                    startingAt: track,
                                    shuffled: true
                                )
                            }
                        } label: {
                            Label("Shuffle from Here", systemImage: "shuffle")
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
                    }
                }

                if model.tracks.isEmpty, !model.isBusy {
                    Text("This playlist is empty.")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle(playlist.name.phoneCompacted)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    model.presentPlaylistSettings(playlist)
                } label: {
                    Label("Edit Playlist", systemImage: "ellipsis.circle")
                }
            }
        }
        .task(id: playlist.id) {
            if model.activePlaylistName != playlist.name {
                await model.showPlaylist(playlist)
            }
        }
    }
}

private struct PhoneTrackActionPanel: View {
    @ObservedObject var model: AppModel
    let track: TrackItem
    let requestAddToPlaylist: () -> Void
    let requestTrackCover: () -> Void
    let requestAlbumCover: () -> Void
    let exportView: () -> Void

    var body: some View {
        VStack(spacing: 12) {
            HStack {
                Picker("Rating", selection: ratingBinding) {
                    Text("Unrated").tag(0)
                    ForEach(1...10, id: \.self) { value in
                        Text("\(value)/10").tag(value)
                    }
                }
                .pickerStyle(.menu)

                Spacer()

                if viewChoices.count > 1 {
                    Picker("View", selection: viewBinding) {
                        ForEach(viewChoices) { choice in
                            Text(choice.track.viewName ?? (choice.track.isPrimaryView ? "Primary" : "View \(choice.index + 1)"))
                                .tag(choice.id)
                        }
                    }
                    .pickerStyle(.menu)
                }
            }

            Grid(horizontalSpacing: 12, verticalSpacing: 12) {
                GridRow {
                    Button {
                        Task { await model.playNext(track) }
                    } label: {
                        Label("Play Next", systemImage: "text.line.first.and.arrowtriangle.forward")
                    }

                    Button {
                        Task { await model.addToQueue(track) }
                    } label: {
                        Label("Queue", systemImage: "text.badge.plus")
                    }
                }

                GridRow {
                    Button {
                        requestAddToPlaylist()
                    } label: {
                        Label("Playlist", systemImage: "music.note.list")
                    }

                    Button {
                        model.selectTrack(id: track.id)
                        model.presentViewEdit()
                    } label: {
                        Label("Edit View", systemImage: "pencil")
                    }

                    Button {
                        requestTrackCover()
                    } label: {
                        Label("Track Cover", systemImage: "photo")
                    }
                }

                GridRow {
                    Button {
                        requestAlbumCover()
                    } label: {
                        Label("Album Cover", systemImage: "rectangle.stack.badge.plus")
                    }
                    .disabled(!track.hasAlbumIdentity)

                    Button {
                        exportView()
                    } label: {
                        Label("Export", systemImage: "square.and.arrow.up")
                    }
                }
            }
            .buttonStyle(.bordered)
        }
        .padding()
        .background(Color(.secondarySystemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
    }

    private var ratingBinding: Binding<Int> {
        Binding(
            get: {
                if model.detailTrack?.id == track.id {
                    return model.detailTrack?.rating ?? 0
                }
                return track.rating ?? 0
            },
            set: { value in
                model.selectTrack(id: track.id)
                Task { await model.setRating(value == 0 ? nil : value) }
            }
        )
    }

    private var viewChoices: [TrackViewChoice] {
        guard model.detailTrack?.primaryViewID == track.primaryViewID else {
            return []
        }
        return model.detailViewChoices
    }

    private var viewBinding: Binding<String> {
        Binding(
            get: {
                if model.detailTrack?.primaryViewID == track.primaryViewID {
                    return model.detailTrack?.id ?? track.id
                }
                return track.id
            },
            set: { model.selectDetailView(id: $0) }
        )
    }
}

private struct PhoneLyricsNotesPanel: View {
    let details: TrackDetails?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            if let lyricsText = details?.lyricsText,
               !lyricsText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Label("Lyrics", systemImage: "text.quote")
                        .font(.headline)
                    Text(lyricsText)
                        .font(.callout)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }

            if let notes = details?.notes,
               !notes.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Label("Notes", systemImage: "note.text")
                        .font(.headline)
                    Text(notes)
                        .font(.callout)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct PhoneTrackEditSheet: View {
    @ObservedObject var model: AppModel
    let chooseArtwork: () -> Void
    let chooseLyrics: () -> Void

    var body: some View {
        NavigationStack {
            Form {
                Section("View") {
                    TextField("Name", text: $model.viewEditNameDraft)
                }

                Section("Music") {
                    TextField("Title", text: $model.viewEditTitleDraft)
                    TextField("Artist", text: $model.viewEditArtistDraft)
                    TextField("Album", text: $model.viewEditAlbumDraft)
                }

                Section("Artwork") {
                    Button {
                        chooseArtwork()
                    } label: {
                        Label(artworkName, systemImage: "photo")
                    }
                }

                Section("Lyrics") {
                    Button {
                        chooseLyrics()
                    } label: {
                        Label(lyricsName, systemImage: "text.quote")
                    }
                }

                Section("Notes") {
                    TextEditor(text: $model.viewEditNotesDraft)
                        .frame(minHeight: 140)
                }
            }
            .navigationTitle("Edit View")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelViewEdit()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        Task { await model.saveViewEdit() }
                    }
                    .disabled(!canSave)
                }
            }
        }
        .interactiveDismissDisabled(model.isViewSaving)
    }

    private var canSave: Bool {
        !model.isViewSaving
            && model.viewEditChanged
            && !model.viewEditTitleDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var artworkName: String {
        model.viewEditArtworkURL?.lastPathComponent
            ?? model.detailDetails?.artworkURL?.lastPathComponent
            ?? "Choose Artwork"
    }

    private var lyricsName: String {
        model.viewEditLyricsURL?.lastPathComponent
            ?? model.detailDetails?.lyricsURL?.lastPathComponent
            ?? "Choose Lyrics"
    }
}

private struct PhonePlaylistCreateSheet: View {
    @ObservedObject var model: AppModel

    var body: some View {
        NavigationStack {
            Form {
                Section("Playlist") {
                    TextField("Name", text: $model.newPlaylistNameDraft)
                }
            }
            .navigationTitle("New Playlist")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelCreatePlaylist()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") {
                        Task { await model.createPlaylist() }
                    }
                    .disabled(model.newPlaylistNameDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
    }
}

private struct PhonePlaylistPickerSheet: View {
    @ObservedObject var model: AppModel

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(model.playlists) { playlist in
                        Button {
                            Task { await model.addPlaylistPickerTrack(to: playlist) }
                        } label: {
                            HStack(spacing: 12) {
                                PhoneArtworkImage(
                                    artworkURL: playlist.artworkURL,
                                    placeholderSystemImage: "music.note.house",
                                    size: 38,
                                    cornerRadius: 7
                                )

                                VStack(alignment: .leading, spacing: 2) {
                                    Text(playlist.name)
                                        .foregroundStyle(.primary)
                                    Text("\(playlist.trackCount) tracks")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                }
            }
            .overlay {
                if model.playlists.isEmpty {
                    PhoneEmptyState(
                        title: "No Playlists",
                        message: model.status,
                        systemImage: "music.note.house"
                    )
                }
            }
            .navigationTitle("Add to Playlist")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelPlaylistPicker()
                    }
                }

                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        model.presentCreatePlaylist()
                    } label: {
                        Label("New Playlist", systemImage: "plus")
                    }
                }
            }
        }
        .task {
            await model.refreshPlaylists()
        }
    }
}

private struct PhonePlaylistSettingsSheet: View {
    @ObservedObject var model: AppModel
    let chooseArtwork: () -> Void

    var body: some View {
        NavigationStack {
            Form {
                Section("Playlist") {
                    TextField("Name", text: $model.playlistSettingsNameDraft)
                }

                Section("Cover") {
                    Button {
                        chooseArtwork()
                    } label: {
                        Label(artworkName, systemImage: "photo")
                    }
                }
            }
            .navigationTitle("Playlist")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelPlaylistSettings()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        Task { await model.savePlaylistSettings() }
                    }
                    .disabled(!model.playlistSettingsChanged)
                }
            }
        }
    }

    private var artworkName: String {
        model.playlistSettingsArtworkURL?.lastPathComponent
            ?? model.playlistSettingsCurrentArtworkURL?.lastPathComponent
            ?? "Choose Cover"
    }
}

private struct PhoneTrackRow: View {
    let track: TrackItem
    let isCurrent: Bool
    let isPlaying: Bool

    var body: some View {
        HStack(spacing: 12) {
            PhoneArtworkImage(
                artworkURL: track.artworkURL,
                placeholderSystemImage: isPlaying ? "speaker.wave.2.fill" : "music.note",
                size: 46,
                cornerRadius: 8
            )

            VStack(alignment: .leading, spacing: 3) {
                Text(track.phoneDisplayTitle)
                    .font(.body.weight(isCurrent ? .semibold : .regular))
                    .fixedSize(horizontal: false, vertical: true)
                Text(track.phoneDisplaySubtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .layoutPriority(1)

            Text(track.durationText)
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
                .fixedSize()
        }
        .padding(.vertical, 4)
    }
}

private extension String {
    var phoneCompacted: String {
        PhoneDisplayText.compact(self)
    }
}

private extension TrackItem {
    var phoneDisplayTitle: String {
        title.phoneCompacted
    }

    var phoneDisplaySubtitle: String {
        subtitle.phoneCompacted
    }
}

private struct PhoneArtworkImage: View {
    let artworkURL: URL?
    let placeholderSystemImage: String
    let size: CGFloat
    let cornerRadius: CGFloat

    var body: some View {
        ZStack {
            if let artworkURL,
               let image = UIImage(contentsOfFile: artworkURL.path) {
                Image(uiImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
            } else {
                Image(systemName: placeholderSystemImage)
                    .font(.system(size: max(18, size * 0.28), weight: .medium))
                    .foregroundStyle(.secondary)
            }
        }
        .frame(width: size, height: size)
        .background(Color(.secondarySystemBackground))
        .clipShape(RoundedRectangle(cornerRadius: cornerRadius))
    }
}

private struct PhoneEmptyState: View {
    let title: String
    let message: String
    let systemImage: String

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: systemImage)
                .font(.system(size: 44))
                .foregroundStyle(.secondary)
            Text(title)
                .font(.headline)
            Text(message)
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .padding()
    }
}
#endif
