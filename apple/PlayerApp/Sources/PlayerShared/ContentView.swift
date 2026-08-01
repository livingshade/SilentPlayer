#if os(macOS)
import AppKit
import Foundation
import SwiftUI

public struct ContentView: View {
    @ObservedObject private var model: AppModel
    @Environment(\.scenePhase) private var scenePhase
    @SceneStorage("ContentView.sceneSession.v1") private var sceneSession = ""
    @State private var isRestoringPresentation = true
    @State private var pendingSeekProgress: Double?
    @State private var pendingSingleClick: DispatchWorkItem?
    @State private var isFileChecksExpanded = false
    @State private var isZeroOutConfirmationPresented = false
    @State private var isQueuePresented = false
    @State private var isLibraryInformationPresented = false
    @State private var isNowPlayingExpanded = false
    @State private var splitViewVisibility: NavigationSplitViewVisibility = .all
    @State private var pendingLibraryDeletion: TrackItem?
    @State private var isLibraryDeletionConfirmationPresented = false
    private let chooseFolder: () async -> URL?
    private let chooseArtworkFile: () async -> URL?
    private let chooseLyricsFile: () async -> URL?
    private let chooseExportFile: (TrackItem) async -> URL?
    private let chooseLibraryExportPackage: () async -> URL?
    private let chooseLibraryImportPackage: () async -> URL?

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
        .sheet(isPresented: $model.isTrackEditPresented) {
            TrackEditSheet(
                model: model,
                chooseArtworkFile: chooseArtworkFile,
                chooseLyricsFile: chooseLyricsFile
            )
        }
        .sheet(isPresented: $model.isPlaylistCreatePresented) {
            PlaylistCreateSheet(model: model)
        }
        .sheet(isPresented: $model.isPlaylistSettingsPresented) {
            PlaylistSettingsSheet(
                model: model,
                chooseArtworkFile: chooseArtworkFile
            )
        }
        .sheet(isPresented: $isQueuePresented) {
            PlaybackQueueSheet(model: model)
        }
        .sheet(isPresented: $isLibraryInformationPresented) {
            LibraryInformationSheet(
                status: model.status,
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
        .onChange(of: model.libraryScope) { _ in
            persistPresentation()
        }
        .onChange(of: model.selectedTrack?.id) { _ in
            persistPresentation()
        }
        .onChange(of: model.nowPlaying?.id) { trackID in
            if trackID == nil {
                dismissExpandedNowPlaying()
            }
        }
        .onChange(of: model.playlists) { _ in
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

    private var playerContent: some View {
        NavigationSplitView(columnVisibility: $splitViewVisibility) {
            sidebar
                .navigationSplitViewColumnWidth(min: 220, ideal: 270, max: 340)
        } detail: {
            GeometryReader { proxy in
                detailPane(layout: DetailPaneLayout(containerSize: proxy.size))
            }
        }
    }

    @MainActor
    private func restorePresentation() async {
        let requestedSnapshot = MacPresentationPersistence.decode(sceneSession) ?? .initial
        await model.bootstrap(
            restoring: requestedSnapshot.contentScope,
            preferredSelectedTrackID: requestedSnapshot.selectedTrackID
        )
        isRestoringPresentation = false
        persistPresentation()
    }

    private func persistPresentation() {
        guard !isRestoringPresentation else {
            return
        }
        let snapshot = MacPresentationSnapshot(
            contentScope: model.restorableLibraryScope,
            selectedTrackID: model.selectedTrack?.id
        )
        guard let encoded = MacPresentationPersistence.encode(snapshot) else {
            return
        }
        sceneSession = encoded
    }

    private func toggleExpandedNowPlaying() {
        if isNowPlayingExpanded {
            dismissExpandedNowPlaying()
        } else {
            presentExpandedNowPlaying()
        }
    }

    private func presentExpandedNowPlaying() {
        guard model.nowPlaying != nil else {
            return
        }
        isNowPlayingExpanded = true
    }

    private func dismissExpandedNowPlaying() {
        guard isNowPlayingExpanded else {
            return
        }
        isNowPlayingExpanded = false
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

    private func detailPane(layout: DetailPaneLayout) -> some View {
        ZStack {
            VStack(spacing: 0) {
                if isNowPlayingExpanded, let track = model.nowPlaying {
                    expandedNowPlaying(for: track)
                        .layoutPriority(1)
                    Divider()
                    playerBar
                        .fixedSize(horizontal: false, vertical: true)
                } else {
                    contentHeader
                    Divider()
                    if let track = model.detailTrack {
                        nowPlayingPanel(for: track, layout: layout)
                        Divider()
                    }
                    trackList
                        .layoutPriority(1)
                    Divider()
                    playerBar
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            if model.isBusy {
                busyOverlay
            }
        }
    }

    private var busyOverlay: some View {
        ZStack {
            Color.black.opacity(0.12)
                .ignoresSafeArea()
            VStack(spacing: 10) {
                ProgressView()
                    .controlSize(.large)
                Text(model.status)
                    .font(.callout.weight(.medium))
                    .lineLimit(2)
                    .multilineTextAlignment(.center)
            }
            .padding(18)
            .frame(width: 260)
            .background(.regularMaterial)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .shadow(radius: 14, y: 4)
        }
        .allowsHitTesting(true)
    }

    private struct DetailPaneLayout {
        let containerSize: CGSize

        var detailPanelHeight: CGFloat {
            let adaptiveHeight = containerSize.height * 0.34
            return min(max(adaptiveHeight, 220), 360)
        }

        var artworkSize: CGFloat {
            min(max(detailPanelHeight - 76, 144), 220)
        }
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Silent")
                    .font(.title2.weight(.semibold))
                Text(model.libraryScope.title)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            VStack(spacing: 6) {
                scopeButton("Library", icon: "music.note.list", selected: model.libraryScope == .library) {
                    await model.showLibrary()
                }
                scopeButton("History", icon: "clock.arrow.circlepath", selected: model.libraryScope == .history) {
                    await model.showHistory()
                }
            }

            Divider()

            HStack {
                Text("Playlists")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    model.presentCreatePlaylist()
                } label: {
                    Image(systemName: "plus")
                }
                .buttonStyle(.borderless)
                .help("Create playlist")
            }

            ScrollView {
                VStack(spacing: 4) {
                    ForEach(model.playlists) { playlist in
                        playlistButton(playlist)
                    }
                }
            }

            Spacer()

            VStack(alignment: .leading, spacing: 5) {
                Text(model.status)
                    .font(.callout)
                    .foregroundStyle(model.isBusy ? Color.orange : Color.secondary)
                    .lineLimit(2)
                if model.isLibraryWorking || !model.libraryStatus.isEmpty {
                    libraryProgress
                }
                if model.isAnalyzing || !model.analyzeStatus.isEmpty {
                    analyzerProgress
                }
            }
        }
        .padding()
        .frame(minWidth: 220, idealWidth: 270, maxWidth: 340)
    }

    private var contentHeader: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 2) {
                Text(model.libraryScope.title)
                    .font(.title3.weight(.semibold))
                    .lineLimit(1)
                Text("\(model.tracks.count) songs")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 12)

            if supportsSongSearch {
                HStack(spacing: 7) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(.secondary)

                    TextField("Search songs", text: $model.query)
                        .textFieldStyle(.plain)
                        .onSubmit {
                            Task { await model.search() }
                        }

                    if !model.query.isEmpty {
                        Button {
                            clearSearch()
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.borderless)
                        .help("Clear search")
                    }
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 7)
                .frame(width: 280)
                .background(Color(nsColor: .controlBackgroundColor))
                .clipShape(RoundedRectangle(cornerRadius: 7))
                .overlay {
                    RoundedRectangle(cornerRadius: 7)
                        .stroke(Color(nsColor: .separatorColor).opacity(0.55), lineWidth: 1)
                }
            }

            Button {
                Task { await model.playAllVisible() }
            } label: {
                Label("Play All", systemImage: "play.fill")
                    .labelStyle(.iconOnly)
            }
            .buttonStyle(.borderless)
            .disabled(model.tracks.isEmpty)
            .help("Play all songs in \(model.libraryScope.title)")

            Menu {
                contentActionsMenu
            } label: {
                Label("More", systemImage: "ellipsis.circle")
                    .labelStyle(.iconOnly)
            }
            .menuStyle(.borderlessButton)
            .help("More actions")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    @ViewBuilder
    private var contentActionsMenu: some View {
        Button {
            if model.isLibraryWorking {
                model.stopLibraryWork()
            } else {
                Task { await model.reloadActiveScope() }
            }
        } label: {
            Label(
                model.isLibraryWorking ? "Stop Library Task" : "Refresh",
                systemImage: model.isLibraryWorking ? "stop.circle" : "arrow.clockwise"
            )
        }

        Menu {
            playlistSortButton(.defaultOrder)
            Divider()
            playlistSortButton(.title)
            playlistSortButton(.artist)
            playlistSortButton(.album)
            playlistSortButton(.rating)
        } label: {
            Label("Sort By", systemImage: "arrow.up.arrow.down")
        }

        if model.activePlaylistName != nil {
            activePlaylistMenu
        }

        Divider()

        Button {
            Task {
                if let folder = await chooseFolder() {
                    await model.importFolder(folder)
                }
            }
        } label: {
            Label("Import Music…", systemImage: "folder.badge.plus")
        }
        .disabled(model.isLibraryWorking)

        Button {
            if model.isAnalyzing {
                model.stopAnalyze()
            } else {
                Task { await model.analyzeLibrary() }
            }
        } label: {
            Label(
                model.isAnalyzing ? "Stop Analysis" : "Analyze Library",
                systemImage: model.isAnalyzing ? "stop.circle" : "waveform"
            )
        }

        Button {
            Task { await model.auditDatabase() }
        } label: {
            Label("Audit Library", systemImage: "checklist.checked")
        }
        .disabled(model.isLibraryWorking)

        Menu {
            Button {
                Task {
                    if let packageURL = await chooseLibraryExportPackage() {
                        _ = await model.exportLibrary(to: packageURL)
                    }
                }
            } label: {
                Label("Export Library…", systemImage: "square.and.arrow.up")
            }

            Button {
                Task {
                    if let packageURL = await chooseLibraryImportPackage() {
                        await model.importLibrary(from: packageURL)
                    }
                }
            } label: {
                Label("Import Library…", systemImage: "square.and.arrow.down")
            }

            Divider()

            Button(role: .destructive) {
                isZeroOutConfirmationPresented = true
            } label: {
                Label("Zero Out Library…", systemImage: "trash")
            }
        } label: {
            Label("Library Package", systemImage: "externaldrive")
        }
        .disabled(model.isBusy || model.isLibraryWorking || model.isAnalyzing)

        Divider()

        Button {
            isLibraryInformationPresented = true
        } label: {
            Label("Library Information…", systemImage: "info.circle")
        }
    }

    private var activePlaylistMenu: some View {
        Menu {
            if let playlist = activePlaylist {
                Button {
                    model.presentPlaylistSettings(playlist)
                } label: {
                    Label("Edit Playlist…", systemImage: "pencil")
                }
            }

            Button {
                Task { await model.moveSelectedInActivePlaylist(delta: -1) }
            } label: {
                Label("Move Selected Up", systemImage: "arrow.up")
            }
            .disabled(model.selectedTrack == nil)

            Button {
                Task { await model.moveSelectedInActivePlaylist(delta: 1) }
            } label: {
                Label("Move Selected Down", systemImage: "arrow.down")
            }
            .disabled(model.selectedTrack == nil)

            Button {
                Task { await model.removeSelectedFromActivePlaylist() }
            } label: {
                Label("Remove Selected", systemImage: "minus.circle")
            }
            .disabled(model.selectedTrack == nil)

            Divider()

            Button(role: .destructive) {
                Task { await model.clearActivePlaylist() }
            } label: {
                Label("Clear Playlist", systemImage: "clear")
            }

            Button(role: .destructive) {
                Task { await model.deleteActivePlaylist() }
            } label: {
                Label("Delete Playlist", systemImage: "trash")
            }
        } label: {
            Label("Playlist", systemImage: "music.note.list")
        }
    }

    private var activePlaylist: PlaylistItem? {
        guard let name = model.activePlaylistName else {
            return nil
        }
        return model.playlists.first { $0.name == name }
    }

    private func clearSearch() {
        model.query = ""
        Task { await model.reloadActiveScope() }
    }

    private var supportsSongSearch: Bool {
        switch model.libraryScope {
        case .library, .playlist:
            return true
        case .favorites, .history:
            return false
        }
    }

    private var trackList: some View {
        List(selection: Binding(
            get: { model.selectedTrack?.id },
            set: { id in
                model.selectTrack(id: id)
                persistPresentation()
            }
        )) {
            ForEach(model.tracks) { track in
                trackRow(for: track)
            }
        }
        .overlay {
            if model.tracks.isEmpty {
                VStack(spacing: 10) {
                    Image(systemName: emptyIcon)
                        .font(.system(size: 42))
                        .foregroundStyle(.secondary)
                    Text(model.libraryScope.title)
                        .font(.title3.weight(.semibold))
                    Text(model.status)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private var playerBar: some View {
        VStack(spacing: 9) {
            HStack(spacing: 12) {
                Button {
                    guard model.nowPlaying != nil else {
                        return
                    }
                    toggleExpandedNowPlaying()
                } label: {
                    HStack(spacing: 12) {
                        if let track = model.nowPlaying {
                            TrackArtworkThumbnail(
                                artworkURL: track.artworkURL,
                                isCurrent: true,
                                isPlaying: model.isPlaying,
                                hasArtworkHint: track.artworkCount > 0
                            )
                        } else {
                            Image(systemName: "music.note")
                                .foregroundStyle(.secondary)
                                .frame(width: 34, height: 34)
                                .background(Color(nsColor: .separatorColor).opacity(0.18))
                                .clipShape(RoundedRectangle(cornerRadius: 5))
                        }

                        VStack(alignment: .leading, spacing: 3) {
                            Text(model.nowPlaying?.title ?? "Nothing playing")
                                .font(.headline)
                                .lineLimit(1)
                            Text(model.nowPlaying?.subtitle ?? "Choose a song to start listening")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }

                        if model.nowPlaying != nil {
                            Image(systemName: isNowPlayingExpanded ? "chevron.down" : "chevron.up")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(model.nowPlaying == nil)
                .frame(maxWidth: .infinity, alignment: .leading)
                .help(isNowPlayingExpanded ? "Close Now Playing" : "Open Now Playing")

                HStack(spacing: 12) {
                    Button {
                        Task { await model.toggleShuffle() }
                    } label: {
                        Label("Shuffle", systemImage: "shuffle")
                            .labelStyle(.iconOnly)
                            .foregroundStyle(model.isShuffleEnabled ? Color.accentColor : Color.secondary)
                    }
                    .help(model.isShuffleEnabled ? "Shuffle on" : "Shuffle off")

                    Button {
                        Task { await model.previousTrack() }
                    } label: {
                        Label("Previous", systemImage: "backward.fill")
                            .labelStyle(.iconOnly)
                    }
                    .help("Previous")

                    Button {
                        Task { await model.pauseOrResume() }
                    } label: {
                        Label(model.isPlaying ? "Pause" : "Play", systemImage: model.isPlaying ? "pause.fill" : "play.fill")
                            .labelStyle(.iconOnly)
                            .frame(width: 22)
                    }
                    .keyboardShortcut(.space, modifiers: [])
                    .help(model.isPlaying ? "Pause" : "Play")

                    Button {
                        Task { await model.nextTrack() }
                    } label: {
                        Label("Next", systemImage: "forward.fill")
                            .labelStyle(.iconOnly)
                    }
                    .help("Next")
                }
                .buttonStyle(.borderless)

                Menu {
                    ForEach(PlaybackRepeatMode.allCases) { mode in
                        Button {
                            Task { await model.setRepeatMode(mode) }
                        } label: {
                            Label(mode.label, systemImage: model.repeatMode == mode ? "checkmark" : mode.systemImage)
                        }
                    }
                } label: {
                    Label(model.repeatMode.label, systemImage: model.repeatMode.systemImage)
                        .foregroundStyle(model.repeatMode == .off ? Color.secondary : Color.accentColor)
                }
                .menuStyle(.borderlessButton)
                .help("Repeat mode")

                Button {
                    Task { await model.addSelectedToPlaylist() }
                } label: {
                    Label("Add to Playlist", systemImage: "text.badge.plus")
                        .labelStyle(.iconOnly)
                }
                .buttonStyle(.borderless)
                .disabled(model.selectedTrack == nil)
                .help("Add to playlist")

                Button {
                    isQueuePresented = true
                } label: {
                    Label(model.queueStatusText, systemImage: "music.note.list")
                        .font(.callout.weight(.semibold))
                        .padding(.horizontal, 3)
                }
                .buttonStyle(.bordered)
                .controlSize(.regular)
                .help("Show queue")

                Button {
                    toggleExpandedNowPlaying()
                } label: {
                    Label(
                        isNowPlayingExpanded ? "Close Now Playing" : "Open Now Playing",
                        systemImage: isNowPlayingExpanded ? "rectangle.compress.vertical" : "rectangle.expand.vertical"
                    )
                    .labelStyle(.iconOnly)
                    .foregroundStyle(isNowPlayingExpanded ? Color.accentColor : Color.secondary)
                }
                .buttonStyle(.borderless)
                .disabled(model.nowPlaying == nil)
                .help(isNowPlayingExpanded ? "Close Now Playing" : "Open Now Playing")

                if model.isBusy {
                    ProgressView()
                        .controlSize(.small)
                }
            }

            HStack(spacing: 10) {
                Text(model.playbackTimeText)
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(width: 92, alignment: .leading)

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
            }

            if !model.playbackError.isEmpty {
                HStack {
                    Label(model.playbackError, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption2)
                        .foregroundStyle(.red)
                        .lineLimit(2)
                        .textSelection(.enabled)
                    Spacer()
                }
            }
        }
        .padding()
        .background(.bar)
    }

    private func nowPlayingPanel(for track: TrackItem, layout: DetailPaneLayout) -> some View {
        HStack(alignment: .top, spacing: 18) {
            ArtworkViewport(
                artworkURL: model.nowPlayingDetails?.artworkURL,
                size: layout.artworkSize
            )
            .frame(width: layout.artworkSize)

            ScrollView(.vertical) {
                VStack(alignment: .leading, spacing: 10) {
                    HStack(alignment: .top, spacing: 12) {
                        VStack(alignment: .leading, spacing: 5) {
                            Text(track.title)
                                .font(.title3.weight(.semibold))
                                .lineLimit(2)
                            Text(track.subtitle)
                                .font(.callout)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                            HStack(spacing: 12) {
                                Label(track.durationText, systemImage: "clock")
                                Label(track.gainText, systemImage: "speaker.wave.2")
                                playbackStatusLabel(for: track)
                            }
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }

                        Spacer(minLength: 8)

                        VStack(alignment: .trailing, spacing: 8) {
                            ratingPicker(for: track)
                                .frame(maxWidth: 140, alignment: .trailing)
                            trackActionsMenu(for: track)
                        }
                    }

                    secondaryContentPanels
                    fileDetailsPanel
                }
                .padding(.vertical, 1)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .frame(height: layout.detailPanelHeight, alignment: .topLeading)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.55))
    }

    private func expandedNowPlaying(for track: TrackItem) -> some View {
        let details = playbackDetails(for: track)
        let artworkURL = details?.artworkURL ?? track.artworkURL
        return ZStack {
            NowPlayingBackdrop(artworkURL: artworkURL)

            GeometryReader { proxy in
                let leftWidth = min(max(proxy.size.width * 0.41, 290), 430)
                let notesHeight = min(max(proxy.size.height * 0.23, 118), 168)

                HStack(alignment: .top, spacing: 22) {
                    ViewThatFits(in: .vertical) {
                        expandedDetailColumn(
                            for: track,
                            details: details,
                            artworkSize: min(210, proxy.size.height * 0.29)
                        )
                        expandedDetailColumn(
                            for: track,
                            details: details,
                            artworkSize: 132
                        )
                        expandedDetailColumn(
                            for: track,
                            details: details,
                            artworkSize: 92
                        )
                    }
                    .frame(width: leftWidth)
                    .frame(maxHeight: .infinity, alignment: .top)

                    Divider()

                    VStack(alignment: .leading, spacing: 12) {
                        HStack {
                            Label("Lyrics", systemImage: "text.quote")
                                .font(.headline)
                            Spacer()
                            if let format = details?.lyricsDocument?.format {
                                Text(format == .lrc ? "Synced" : "Plain Text")
                                    .font(.caption2.weight(.medium))
                                    .foregroundStyle(.secondary)
                            }
                            Button {
                                dismissExpandedNowPlaying()
                            } label: {
                                Label("Close Now Playing", systemImage: "xmark")
                                    .labelStyle(.iconOnly)
                                    .frame(width: 24, height: 24)
                            }
                            .buttonStyle(.bordered)
                            .keyboardShortcut(.cancelAction)
                            .help("Close Now Playing")
                        }

                        NowPlayingLyricsView(
                            model: model,
                            document: details?.lyricsDocument,
                            fallbackText: details?.lyricsText,
                            isLoading: model.isLoadingPlaybackDetails
                        )
                        .id(track.id)
                        .layoutPriority(1)

                        Divider()

                        expandedNotes(for: track, details: details)
                            .frame(height: notesHeight, alignment: .top)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
                .padding(.horizontal, 22)
                .padding(.vertical, 18)
            }
        }
    }

    private func expandedDetailColumn(
        for track: TrackItem,
        details: TrackDetails?,
        artworkSize: CGFloat
    ) -> some View {
        VStack(spacing: 10) {
            ArtworkViewport(
                artworkURL: details?.artworkURL ?? track.artworkURL,
                size: artworkSize
            )

            VStack(spacing: 4) {
                Text(track.title)
                    .font(.title2.weight(.semibold))
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                Text(track.artist)
                    .font(.title3.weight(.medium))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                Text(track.album)
                    .font(.callout)
                    .foregroundStyle(.tertiary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity)

            HStack(spacing: 12) {
                ratingPicker(for: track)
                    .frame(maxWidth: 150)
                Spacer(minLength: 4)
                trackActionsMenu(for: track)
            }

            expandedTrackFacts(for: track, details: details)
            expandedPlaybackHistory(details: details)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }

    private func expandedPlaybackProgress(for track: TrackItem) -> some View {
        VStack(spacing: 7) {
            TimelineView(.periodic(from: .now, by: model.isPlaying ? 0.2 : 1)) { _ in
                let positionMS = model.estimatedPlaybackPositionMS()
                HStack {
                    Text(playbackTimestamp(positionMS))
                    Spacer()
                    Text(track.durationText)
                }
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
            }

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
            .disabled(track.durationMS == nil)
        }
    }

    private var expandedPlaybackControls: some View {
        HStack(spacing: 22) {
            Button {
                Task { await model.toggleShuffle() }
            } label: {
                Label("Shuffle", systemImage: "shuffle")
                    .labelStyle(.iconOnly)
                    .foregroundStyle(model.isShuffleEnabled ? Color.accentColor : Color.secondary)
            }
            .buttonStyle(.borderless)
            .help(model.isShuffleEnabled ? "Shuffle on" : "Shuffle off")

            Button {
                Task { await model.previousTrack() }
            } label: {
                Label("Previous", systemImage: "backward.fill")
                    .labelStyle(.iconOnly)
                    .font(.title3)
            }
            .buttonStyle(.borderless)
            .help("Previous")

            Button {
                Task { await model.pauseOrResume() }
            } label: {
                Label(
                    model.isPlaying ? "Pause" : "Play",
                    systemImage: model.isPlaying ? "pause.fill" : "play.fill"
                )
                .labelStyle(.iconOnly)
                .font(.title2)
                .frame(width: 30, height: 30)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .clipShape(Circle())
            .help(model.isPlaying ? "Pause" : "Play")

            Button {
                Task { await model.nextTrack() }
            } label: {
                Label("Next", systemImage: "forward.fill")
                    .labelStyle(.iconOnly)
                    .font(.title3)
            }
            .buttonStyle(.borderless)
            .help("Next")

            Menu {
                ForEach(PlaybackRepeatMode.allCases) { mode in
                    Button {
                        Task { await model.setRepeatMode(mode) }
                    } label: {
                        Label(mode.label, systemImage: model.repeatMode == mode ? "checkmark" : mode.systemImage)
                    }
                }
            } label: {
                Label(model.repeatMode.label, systemImage: model.repeatMode.systemImage)
                    .labelStyle(.iconOnly)
                    .foregroundStyle(model.repeatMode == .off ? Color.secondary : Color.accentColor)
            }
            .menuStyle(.borderlessButton)
            .help("Repeat mode")

            Button {
                isQueuePresented = true
            } label: {
                Label(model.queueStatusText, systemImage: "music.note.list")
                    .labelStyle(.iconOnly)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.borderless)
            .help(model.queueStatusText)
        }
    }

    private func expandedTrackFacts(
        for track: TrackItem,
        details: TrackDetails?
    ) -> some View {
        VStack(alignment: .leading, spacing: 9) {
            Label("Track Details", systemImage: "info.circle")
                .font(.headline)

            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 7) {
                GridRow {
                    Label(track.durationText, systemImage: "clock")
                    Label(details?.formatName ?? track.formatName ?? "Unknown format", systemImage: "waveform")
                }
                GridRow {
                    Label(details?.qualityProfile ?? track.qualityProfile ?? "Quality not set", systemImage: "hifispeaker")
                    Label(track.gainText, systemImage: "speaker.wave.2")
                }
                GridRow {
                    Label(track.ratingText, systemImage: track.rating == nil ? "star" : "star.fill")
                    Label(model.isPlaying ? "Playing" : "Paused", systemImage: model.isPlaying ? "waveform" : "pause.circle")
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .lineLimit(1)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .textBackgroundColor).opacity(0.65))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func expandedPlaybackHistory(details: TrackDetails?) -> some View {
        VStack(alignment: .leading, spacing: 9) {
            Label("Listening History", systemImage: "clock.arrow.circlepath")
                .font(.headline)

            Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 7) {
                GridRow {
                    LabeledContent("Plays", value: "\(details?.playCount ?? 0)")
                    LabeledContent("Sessions", value: "\(details?.playbackSessionCount ?? 0)")
                }
                GridRow {
                    LabeledContent(
                        "Last Played",
                        value: playbackDateText(details?.lastPlayedAtUnixSeconds)
                    )
                    LabeledContent(
                        "Last Completed",
                        value: playbackDateText(details?.lastCompletedAtUnixSeconds)
                    )
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .textBackgroundColor).opacity(0.65))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func expandedNotes(
        for track: TrackItem,
        details: TrackDetails?
    ) -> some View {
        let notes = details?.notes?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return VStack(alignment: .leading, spacing: 8) {
            HStack {
                Label("Notes", systemImage: "note.text")
                    .font(.headline)
                Spacer()
                Button {
                    model.presentTrackEdit(for: track)
                } label: {
                    Label("Edit Notes", systemImage: "pencil")
                }
                .buttonStyle(.borderless)
                .disabled(model.isLoadingPlaybackDetails)
            }

            Text(notes.isEmpty ? "No notes" : notes)
                .font(.callout)
                .foregroundStyle(notes.isEmpty ? Color.secondary : Color.primary)
                .lineLimit(4)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .textBackgroundColor).opacity(0.65))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func playbackDateText(_ unixSeconds: Int64?) -> String {
        guard let unixSeconds else {
            return "Never"
        }
        return Date(timeIntervalSince1970: TimeInterval(unixSeconds))
            .formatted(date: .abbreviated, time: .shortened)
    }

    private func playbackTimestamp(_ milliseconds: Int) -> String {
        let totalSeconds = max(0, milliseconds / 1_000)
        return "\(totalSeconds / 60):\(String(format: "%02d", totalSeconds % 60))"
    }

    private func playbackDetails(for track: TrackItem) -> TrackDetails? {
        guard let details = model.playbackDetails,
              details.identity == track.identity else {
            return nil
        }
        return details
    }

    private func ratingPicker(for track: TrackItem) -> some View {
        Picker(
            selection: Binding(
                get: { model.detailTrack?.rating ?? 0 },
                set: { value in
                    Task { await model.setRating(value == 0 ? nil : value) }
                }
            )
        ) {
            Text("Unrated").tag(0)
            ForEach(1...10, id: \.self) { value in
                Text("\(value)/10").tag(value)
            }
        } label: {
            Label(track.ratingText, systemImage: track.rating == nil ? "star" : "star.fill")
        }
        .pickerStyle(.menu)
        .help("Set rating")
    }

    private func trackActionsMenu(for track: TrackItem) -> some View {
        Menu {
            Button {
                setTrackCover(for: track)
            } label: {
                Label("Set Track Cover", systemImage: "photo")
            }

            Button {
                setAlbumCover(for: track)
            } label: {
                Label("Set Album Cover", systemImage: "rectangle.stack.badge.plus")
            }
            .disabled(!track.hasAlbumIdentity)

            Divider()

            Button {
                model.presentTrackEdit()
            } label: {
                Label("Edit Song…", systemImage: "pencil")
            }
            .disabled(model.isLoadingDetails || model.detailTrack == nil)

            Button {
                materialize(track)
            } label: {
                Label("Export Song…", systemImage: "square.and.arrow.down")
            }
            .disabled(model.detailTrack == nil)
        } label: {
            Label("Track Actions", systemImage: "ellipsis.circle")
                .labelStyle(.iconOnly)
        }
        .menuStyle(.borderlessButton)
        .help("Track actions")
    }

    @ViewBuilder
    private var secondaryContentPanels: some View {
        let notes = model.detailDetails?.notes?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        lyricsPanel
        if !notes.isEmpty {
            notesPanel
        }
    }

    private var fileDetailsPanel: some View {
        Group {
            if let details = model.nowPlayingDetails {
                let errorDiagnostics = details.diagnostics.filter { $0.severity == .error }
                let optionalDiagnostics = details.diagnostics.filter { $0.severity != .error }

                VStack(alignment: .leading, spacing: 8) {
                    if !errorDiagnostics.isEmpty {
                        diagnosticsList(errorDiagnostics)
                    }

                    DisclosureGroup(isExpanded: $isFileChecksExpanded) {
                        VStack(alignment: .leading, spacing: 8) {
                            Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 5) {
                                fileFieldRow("File ID", details.identity)
                                fileFieldRow("Format", optionalFileValue(details.formatName))
                                fileFieldRow("Quality", optionalFileValue(details.qualityProfile))
                                fileFieldRow("Artwork", optionalFileValue(details.artworkSource))
                            }
                            .font(.caption)

                            if !optionalDiagnostics.isEmpty {
                                diagnosticsList(optionalDiagnostics)
                            }
                        }
                        .padding(.top, 4)
                    } label: {
                        Label(
                            "File Details",
                            systemImage: "doc.text.magnifyingglass"
                        )
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    }
                    .disclosureGroupStyle(.automatic)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func diagnosticsList(_ diagnostics: [TrackDiagnostic]) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            ForEach(diagnostics) { diagnostic in
                HStack(alignment: .top, spacing: 6) {
                    Image(systemName: diagnosticIcon(diagnostic.severity))
                        .frame(width: 14)
                        .foregroundStyle(diagnosticColor(diagnostic.severity))
                    VStack(alignment: .leading, spacing: 1) {
                        Text(diagnostic.title)
                            .font(.caption.weight(.medium))
                        Text(diagnostic.detail)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                }
            }
        }
    }

    private func fileFieldRow(_ label: String, _ value: String) -> some View {
        GridRow {
            Text(label)
                .foregroundStyle(.secondary)
            Text(value)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
        }
    }

    private func optionalFileValue(_ value: String?) -> String {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines), !value.isEmpty else {
            return "Not set"
        }
        return value
    }

    private func diagnosticIcon(_ severity: TrackDiagnosticSeverity) -> String {
        switch severity {
        case .error:
            return "xmark.octagon.fill"
        case .warning:
            return "exclamationmark.triangle.fill"
        case .info:
            return "info.circle"
        }
    }

    private func diagnosticColor(_ severity: TrackDiagnosticSeverity) -> Color {
        switch severity {
        case .error:
            return .red
        case .warning:
            return .orange
        case .info:
            return .secondary
        }
    }

    private var lyricsPanel: some View {
        let details = model.detailDetails
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Text("Lyrics")
                    .font(.headline)
                Spacer()
                if let format = details?.lyricsDocument?.format {
                    Text(format == .lrc ? "Synced" : "Plain Text")
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.secondary)
                }
            }

            CompactLyricsView(
                model: model,
                track: model.detailTrack,
                document: details?.lyricsDocument,
                fallbackText: details?.lyricsText,
                isLoading: model.isLoadingDetails
            )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var notesPanel: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Notes")
                .font(.headline)

            if let notes = model.nowPlayingDetails?.notes,
               !notes.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                ScrollView {
                    Text(notes)
                        .font(.callout)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                }
                .frame(maxHeight: 130)
                .background(Color(nsColor: .textBackgroundColor))
                .clipShape(RoundedRectangle(cornerRadius: 6))
            } else {
                Text("No notes")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, minHeight: 74, alignment: .center)
                    .background(Color(nsColor: .textBackgroundColor))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var analyzerProgress: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let progress = model.analyzeProgress {
                ProgressView(value: progress)
                    .controlSize(.small)
            } else if model.isAnalyzing {
                ProgressView()
                    .controlSize(.small)
            }
            Text(model.analyzeStatus)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
    }

    private var libraryProgress: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let progress = model.libraryProgress {
                ProgressView(value: progress)
                    .controlSize(.small)
            } else if model.isLibraryWorking {
                ProgressView()
                    .controlSize(.small)
            }
            Text(model.libraryStatus)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
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

    private func playlistSortButton(_ sortMode: PlaylistSortMode) -> some View {
        Button {
            Task { await model.sortVisibleTracks(sortMode) }
        } label: {
            Label(
                sortMode.label,
                systemImage: model.playlistSortMode == sortMode ? "checkmark" : sortMode.systemImage
            )
        }
    }

    private func playbackStatusLabel(for track: TrackItem) -> some View {
        Group {
            if model.nowPlaying?.id == track.id && model.isPlaying {
                Label("Playing", systemImage: "waveform")
                    .foregroundStyle(Color.green)
            } else if model.nowPlaying?.id == track.id {
                Label("Paused", systemImage: "pause.circle")
                    .foregroundStyle(.secondary)
            } else {
                Label("Selected", systemImage: "info.circle")
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func trackRow(for track: TrackItem) -> some View {
        let isCurrent = model.nowPlaying?.id == track.id
        return TrackRow(track: track, isCurrent: isCurrent, isPlaying: isCurrent && model.isPlaying)
            .tag(track.id)
            .contentShape(Rectangle())
            .onTapGesture(count: 2) {
                playTrackFromRow(track)
            }
            .onTapGesture(count: 1) {
                scheduleTrackSelection(track)
            }
            .contextMenu {
                trackContextMenu(for: track)
            }
    }

    private func scheduleTrackSelection(_ track: TrackItem) {
        pendingSingleClick?.cancel()
        let work = DispatchWorkItem {
            model.selectTrack(id: track.id)
            persistPresentation()
        }
        pendingSingleClick = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.18, execute: work)
    }

    private func selectTrackImmediately(_ track: TrackItem) {
        pendingSingleClick?.cancel()
        pendingSingleClick = nil
        model.selectTrack(id: track.id)
        persistPresentation()
    }

    private func playTrackFromRow(_ track: TrackItem) {
        selectTrackImmediately(track)
        Task { await model.play(track) }
    }

    @ViewBuilder
    private func trackContextMenu(for track: TrackItem) -> some View {
        Button {
            playTrackFromRow(track)
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
            selectTrackImmediately(track)
            Task { await model.addSelectedToPlaylist() }
        } label: {
            Label("Add to Playlist", systemImage: "text.badge.plus")
        }

        Divider()

        Button {
            selectTrackImmediately(track)
            model.presentTrackEdit()
        } label: {
            Label("Edit Song", systemImage: "pencil")
        }

        Button {
            selectTrackImmediately(track)
            setTrackCover(for: track)
        } label: {
            Label("Set Track Cover", systemImage: "photo")
        }

        Button {
            selectTrackImmediately(track)
            setAlbumCover(for: track)
        } label: {
            Label("Set Album Cover", systemImage: "rectangle.stack.badge.plus")
        }
        .disabled(!track.hasAlbumIdentity)

        Button {
            selectTrackImmediately(track)
            materialize(track)
        } label: {
            Label("Export Song", systemImage: "square.and.arrow.down")
        }

        if model.activePlaylistName != nil {
            Divider()

            Button {
                selectTrackImmediately(track)
                Task { await model.moveSelectedInActivePlaylist(delta: -1) }
            } label: {
                Label("Move Up", systemImage: "arrow.up")
            }

            Button {
                selectTrackImmediately(track)
                Task { await model.moveSelectedInActivePlaylist(delta: 1) }
            } label: {
                Label("Move Down", systemImage: "arrow.down")
            }

            Button(role: .destructive) {
                selectTrackImmediately(track)
                Task { await model.removeSelectedFromActivePlaylist() }
            } label: {
                Label("Remove from Playlist", systemImage: "minus.circle")
            }
        }

        Divider()

        Button(role: .destructive) {
            pendingLibraryDeletion = track
            isLibraryDeletionConfirmationPresented = true
        } label: {
            Label("Delete from Library…", systemImage: "trash")
        }
    }

    private func scopeButton(
        _ title: String,
        icon: String,
        selected: Bool,
        action: @escaping () async -> Void
    ) -> some View {
        Button {
            Task {
                await action()
                persistPresentation()
            }
        } label: {
            HStack {
                Label(title, systemImage: icon)
                Spacer()
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .contentShape(Rectangle())
            .background(selected ? Color.accentColor.opacity(0.14) : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
    }

    private func playlistButton(_ playlist: PlaylistItem) -> some View {
        let selected = model.libraryScope == .playlist(playlist.name)
        return Button {
            Task {
                await model.showPlaylist(playlist)
                persistPresentation()
            }
        } label: {
            HStack(spacing: 8) {
                PlaylistArtworkThumbnail(artworkURL: playlist.artworkURL)
                Text(playlist.name)
                    .lineLimit(1)
                Spacer()
                Text("\(playlist.trackCount)")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .contentShape(Rectangle())
            .background(selected ? Color.accentColor.opacity(0.14) : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button {
                model.presentPlaylistSettings(playlist)
            } label: {
                Label("Rename...", systemImage: "pencil")
            }

            Button {
                Task {
                    if let imageURL = await chooseArtworkFile() {
                        await model.setPlaylistArtwork(playlist, imageURL: imageURL)
                    }
                }
            } label: {
                Label("Set Cover...", systemImage: "photo")
            }
        }
    }

    private func materialize(_ track: TrackItem) {
        Task {
            if let destination = await chooseExportFile(track) {
                await model.materializeSelected(to: destination)
            }
        }
    }

    private func setTrackCover(for track: TrackItem) {
        Task {
            if let imageURL = await chooseArtworkFile() {
                await model.setTrackArtwork(for: track, imageURL: imageURL)
            }
        }
    }

    private func setAlbumCover(for track: TrackItem) {
        Task {
            if let imageURL = await chooseArtworkFile() {
                await model.setAlbumArtwork(for: track, imageURL: imageURL)
            }
        }
    }
}

private struct LibraryInformationSheet: View {
    let status: String
    let databasePath: String
    let musicPath: String
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                Text("Library Information")
                    .font(.title2.weight(.semibold))
                Spacer()
                Button("Done") {
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
            }

            Divider()

            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 12) {
                informationRow("Status", status)
                informationRow("Database", databasePath)
                informationRow("Music Folder", musicPath)
            }
        }
        .padding(22)
        .frame(minWidth: 560, idealWidth: 640, maxWidth: 760)
    }

    private func informationRow(_ label: String, _ value: String) -> some View {
        GridRow {
            Text(label)
                .foregroundStyle(.secondary)
            Text(value)
                .font(label == "Status" ? .body : .callout.monospaced())
                .lineLimit(3)
                .truncationMode(.middle)
                .textSelection(.enabled)
        }
    }
}

private struct PlaybackQueueSheet: View {
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
                        Text("Use Play Next or Add to Queue from any track.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, minHeight: 220)
                    .listRowSeparator(.hidden)
                } else {
                    ForEach(Array(model.playbackQueue.enumerated()), id: \.element.id) { index, track in
                        queueRow(track, at: index)
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
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
                ToolbarItem(placement: .destructiveAction) {
                    Button("Clear", role: .destructive) {
                        Task { await model.clearPlaybackQueue() }
                    }
                    .disabled(model.playbackQueue.isEmpty)
                }
            }
        }
        .frame(minWidth: 520, idealWidth: 620, minHeight: 420, idealHeight: 560)
        .task {
            await model.refreshPlaybackState()
        }
    }

    private func queueRow(_ track: TrackItem, at index: Int) -> some View {
        HStack(spacing: 10) {
            Image(systemName: model.queuePosition == index ? "speaker.wave.2.fill" : "line.3.horizontal")
                .foregroundStyle(model.queuePosition == index ? Color.accentColor : Color.secondary)
                .frame(width: 20)

            VStack(alignment: .leading, spacing: 2) {
                Text(track.title)
                    .lineLimit(1)
                Text(track.subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            Button {
                Task { await model.moveQueueItem(from: index, to: index - 1) }
            } label: {
                Image(systemName: "arrow.up")
            }
            .buttonStyle(.borderless)
            .disabled(index == 0)
            .help("Move up")

            Button {
                Task { await model.moveQueueItem(from: index, to: index + 1) }
            } label: {
                Image(systemName: "arrow.down")
            }
            .buttonStyle(.borderless)
            .disabled(index + 1 >= model.playbackQueue.count)
            .help("Move down")

            Button(role: .destructive) {
                Task { await model.removeQueueItem(at: index) }
            } label: {
                Image(systemName: "minus.circle")
            }
            .buttonStyle(.borderless)
            .help("Remove from queue")
        }
        .padding(.vertical, 3)
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

private struct TrackEditSheet: View {
    @ObservedObject var model: AppModel
    let chooseArtworkFile: () async -> URL?
    let chooseLyricsFile: () async -> URL?

    var body: some View {
        NavigationStack {
            Form {
                Section("Music") {
                    TextField("Title", text: $model.trackEditTitleDraft)
                    TextField("Artist", text: $model.trackEditArtistDraft)
                    TextField("Album", text: $model.trackEditAlbumDraft)
                    LabeledContent("Format", value: formatName)
                }

                Section("Artwork") {
                    HStack {
                        Label(selectedArtworkName, systemImage: "photo")
                            .lineLimit(1)
                        Spacer()
                        Button {
                            Task {
                                if let url = await chooseArtworkFile() {
                                    await MainActor.run {
                                        model.setTrackEditArtworkURL(url)
                                    }
                                }
                            }
                        } label: {
                            Label("Choose", systemImage: "folder")
                        }
                    }
                }

                Section("Lyrics") {
                    HStack {
                        Label(selectedLyricsName, systemImage: "text.quote")
                            .lineLimit(1)
                        Spacer()
                        Button {
                            Task {
                                if let url = await chooseLyricsFile() {
                                    await MainActor.run {
                                        model.setTrackEditLyricsURL(url)
                                    }
                                }
                            }
                        } label: {
                            Label("Choose", systemImage: "folder")
                        }
                    }
                }

                Section("Notes") {
                    TextEditor(text: $model.trackEditNotesDraft)
                        .font(.callout)
                        .frame(minHeight: 120)
                }
            }
            .navigationTitle("Edit Song")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelTrackEdit()
                    }
                    .disabled(model.isTrackSaving)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        Task { await model.saveTrackEdit() }
                    }
                    .disabled(!canSave)
                }
            }
        }
        .frame(minWidth: 520, idealWidth: 560, maxWidth: 720, minHeight: 560, idealHeight: 620, maxHeight: 760)
        .interactiveDismissDisabled(model.isTrackSaving)
    }

    private var canSave: Bool {
        !model.isTrackSaving
            && model.trackEditChanged
            && !model.trackEditTitleDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var selectedArtworkName: String {
        model.trackEditArtworkURL?.lastPathComponent
            ?? model.nowPlayingDetails?.artworkURL?.lastPathComponent
            ?? "No Artwork"
    }

    private var selectedLyricsName: String {
        model.trackEditLyricsURL?.lastPathComponent
            ?? model.nowPlayingDetails?.lyricsURL?.lastPathComponent
            ?? "No Lyrics"
    }

    private var formatName: String {
        model.nowPlayingDetails?.formatName?.uppercased()
            ?? model.detailTrack?.formatName?.uppercased()
            ?? "Unknown"
    }

}

private struct PlaylistCreateSheet: View {
    @ObservedObject var model: AppModel

    var body: some View {
        NavigationStack {
            Form {
                Section("Playlist") {
                    TextField("Name", text: $model.newPlaylistNameDraft)
                        .onSubmit {
                            Task { await model.createPlaylist() }
                        }
                }
            }
            .navigationTitle("New Playlist")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelCreatePlaylist()
                    }
                    .disabled(model.isBusy)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") {
                        Task { await model.createPlaylist() }
                    }
                    .disabled(!canCreate)
                }
            }
        }
        .frame(minWidth: 380, idealWidth: 420, maxWidth: 520, minHeight: 180, idealHeight: 220, maxHeight: 300)
        .interactiveDismissDisabled(model.isBusy)
    }

    private var canCreate: Bool {
        !model.isBusy && !model.newPlaylistNameDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

private struct PlaylistSettingsSheet: View {
    @ObservedObject var model: AppModel
    let chooseArtworkFile: () async -> URL?

    var body: some View {
        NavigationStack {
            Form {
                Section("Playlist") {
                    TextField("Name", text: $model.playlistSettingsNameDraft)
                }

                Section("Cover") {
                    HStack(spacing: 10) {
                        PlaylistArtworkThumbnail(artworkURL: previewArtworkURL)
                            .frame(width: 30, height: 30)
                        Text(artworkName)
                            .lineLimit(1)
                        Spacer()
                        Button {
                            Task {
                                if let imageURL = await chooseArtworkFile() {
                                    await MainActor.run {
                                        model.setPlaylistSettingsArtworkURL(imageURL)
                                    }
                                }
                            }
                        } label: {
                            Label("Choose", systemImage: "folder")
                        }
                    }
                }
            }
            .navigationTitle("Playlist")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelPlaylistSettings()
                    }
                    .disabled(model.isBusy)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        Task { await model.savePlaylistSettings() }
                    }
                    .disabled(!canSave)
                }
            }
        }
        .frame(minWidth: 440, idealWidth: 480, maxWidth: 620, minHeight: 260, idealHeight: 320, maxHeight: 460)
        .interactiveDismissDisabled(model.isBusy)
    }

    private var previewArtworkURL: URL? {
        model.playlistSettingsArtworkURL ?? model.playlistSettingsCurrentArtworkURL
    }

    private var artworkName: String {
        if let artworkURL = model.playlistSettingsArtworkURL {
            return artworkURL.lastPathComponent
        }
        if let artworkURL = model.playlistSettingsCurrentArtworkURL {
            return artworkURL.lastPathComponent
        }
        return "No Cover"
    }

    private var canSave: Bool {
        !model.isBusy
            && model.playlistSettingsChanged
            && !model.playlistSettingsNameDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

private struct TrackRow: View {
    let track: TrackItem
    let isCurrent: Bool
    let isPlaying: Bool

    var body: some View {
        HStack(spacing: 12) {
            TrackArtworkThumbnail(
                artworkURL: track.artworkURL,
                isCurrent: isCurrent,
                isPlaying: isPlaying,
                hasArtworkHint: track.artworkCount > 0
            )

            VStack(alignment: .leading, spacing: 3) {
                Text(track.title)
                    .font(.body.weight(.medium))
                    .lineLimit(1)
                Text(track.subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            VStack(alignment: .trailing, spacing: 3) {
                Text(track.durationText)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                HStack(spacing: 3) {
                    Image(systemName: track.rating == nil ? "star" : "star.fill")
                        .font(.caption2)
                    Text(track.ratingText)
                        .font(.caption2.monospacedDigit())
                }
                .foregroundStyle(track.rating == nil ? Color.secondary.opacity(0.65) : Color.accentColor)
                .lineLimit(1)
                Text(track.gainText)
                    .font(.caption2)
                    .foregroundStyle(track.gainDB == nil ? Color.secondary.opacity(0.65) : Color.secondary)
                    .lineLimit(1)
            }
            .frame(width: 96, alignment: .trailing)
        }
        .padding(.vertical, 5)
    }
}

private struct CompactLyricsView: View {
    @ObservedObject var model: AppModel
    let track: TrackItem?
    let document: LyricsDocument?
    let fallbackText: String?
    let isLoading: Bool

    var body: some View {
        Group {
            if let document, document.hasDisplayableLyrics {
                if document.timedLines != nil,
                   let track,
                   model.nowPlaying?.id == track.id {
                    TimelineView(.periodic(from: .now, by: model.isPlaying ? 0.2 : 1)) { _ in
                        lyricLine(document.compactLine(at: model.estimatedPlaybackPositionMS()))
                    }
                } else {
                    lyricLine(document.compactLine())
                }
            } else if let fallbackLine {
                lyricLine(fallbackLine)
            } else if isLoading {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Loading lyrics…")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, minHeight: 62)
            } else {
                NoLyricsState(compact: true)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 62)
        .background(Color(nsColor: .textBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 6))
        .overlay {
            RoundedRectangle(cornerRadius: 6)
                .stroke(Color(nsColor: .separatorColor).opacity(0.28), lineWidth: 1)
        }
    }

    private func lyricLine(_ line: String?) -> some View {
        Text(line ?? "…")
            .font(.callout.weight(.medium))
            .lineLimit(1)
            .minimumScaleFactor(0.82)
            .frame(maxWidth: .infinity, minHeight: 62, alignment: .center)
            .padding(.horizontal, 16)
            .accessibilityLabel(line ?? "Waiting for lyrics")
    }

    private var fallbackLine: String? {
        guard let fallbackText else {
            return nil
        }
        return fallbackText
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty }
    }
}

private struct NoLyricsState: View {
    var compact = false

    var body: some View {
        if compact {
            HStack(spacing: 8) {
                Image(systemName: "text.quote")
                    .foregroundStyle(.secondary)
                Text("No Lyrics")
                    .font(.callout.weight(.medium))
            }
            .frame(maxWidth: .infinity, minHeight: 62)
        } else {
            VStack(spacing: 8) {
                Image(systemName: "text.quote")
                    .font(.title2)
                    .foregroundStyle(.secondary)
                Text("No Lyrics")
                    .font(.headline)
                Text("Add an LRC or text file from Edit Song.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

private struct NowPlayingLyricsView: View {
    @ObservedObject var model: AppModel
    let document: LyricsDocument?
    let fallbackText: String?
    let isLoading: Bool

    var body: some View {
        Group {
            if let document {
                switch document.content {
                case .timed(let lines):
                    if lines.isEmpty {
                        emptyLyrics
                    } else {
                        TimedLyricsView(model: model, document: document)
                    }
                case .plain(let text):
                    plainLyrics(text)
                }
            } else if let fallbackText,
                      !fallbackText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                plainLyrics(fallbackText)
            } else if isLoading {
                VStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Loading lyrics…")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                emptyLyrics
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func plainLyrics(_ text: String) -> some View {
        ScrollView {
            Text(text)
                .font(.title3)
                .lineSpacing(6)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 20)
                .padding(.vertical, 42)
        }
    }

    private var emptyLyrics: some View {
        NoLyricsState()
    }
}

private struct TimedLyricsView: View {
    @ObservedObject var model: AppModel
    let document: LyricsDocument
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var followsPlayback = true

    private var lines: [TimedLyricsLine] {
        document.timedLines ?? []
    }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.2)) { _ in
            let positionMS = model.estimatedPlaybackPositionMS()
            let activeIndex = document.activeLineIndex(at: positionMS)
            lyricsScroller(activeIndex: activeIndex)
        }
    }

    private func lyricsScroller(activeIndex: Int?) -> some View {
        let activeID = activeIndex.map { lines[$0].id }
        return ScrollViewReader { proxy in
            ZStack(alignment: .bottomTrailing) {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 20) {
                        ForEach(lines) { line in
                            lyricButton(
                                line,
                                isActive: line.id == activeID
                            )
                        }
                    }
                    .padding(.horizontal, 20)
                    .padding(.vertical, 56)
                }
                .simultaneousGesture(
                    DragGesture(minimumDistance: 3)
                        .onChanged { _ in
                            followsPlayback = false
                        }
                )

                if !followsPlayback, let activeID {
                    Button {
                        followsPlayback = true
                        scroll(to: activeID, using: proxy)
                    } label: {
                        Label("Follow Lyrics", systemImage: "location.fill")
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .padding(10)
                }
            }
            .onAppear {
                guard followsPlayback, let activeID else {
                    return
                }
                DispatchQueue.main.async {
                    scroll(to: activeID, using: proxy)
                }
            }
            .onChange(of: activeID) { newID in
                guard followsPlayback, let newID else {
                    return
                }
                scroll(to: newID, using: proxy)
            }
        }
    }

    private func lyricButton(
        _ line: TimedLyricsLine,
        isActive: Bool
    ) -> some View {
        Button {
            followsPlayback = true
            Task { await model.seek(toMilliseconds: line.startMS) }
        } label: {
            Text(line.text.isEmpty ? " " : line.text)
                .font(.title3.weight(isActive ? .semibold : .regular))
                .foregroundStyle(isActive ? Color.primary : Color.secondary)
                .multilineTextAlignment(.leading)
                .frame(maxWidth: .infinity, minHeight: 30, alignment: .leading)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(line.text.isEmpty ? "Instrumental" : line.text)
        .accessibilityValue(isActive ? "Current lyric" : "")
        .help("Seek to \(formatLyricsTime(line.startMS))")
    }

    private func scroll(to id: Int, using proxy: ScrollViewProxy) {
        if reduceMotion {
            proxy.scrollTo(id, anchor: .center)
        } else {
            withAnimation(.easeInOut(duration: 0.24)) {
                proxy.scrollTo(id, anchor: .center)
            }
        }
    }

    private func formatLyricsTime(_ milliseconds: Int) -> String {
        let totalSeconds = max(0, milliseconds) / 1_000
        return String(format: "%d:%02d", totalSeconds / 60, totalSeconds % 60)
    }
}

private struct TrackArtworkThumbnail: View {
    let artworkURL: URL?
    let isCurrent: Bool
    let isPlaying: Bool
    let hasArtworkHint: Bool

    var body: some View {
        ZStack {
            #if os(macOS)
            if let artworkURL, let image = NSImage(contentsOf: artworkURL) {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: 34, height: 34)
                    .clipped()
            } else {
                placeholder
            }
            #else
            placeholder
            #endif
        }
        .frame(width: 34, height: 34)
        .background(Color(nsColor: .separatorColor).opacity(0.18))
        .clipShape(RoundedRectangle(cornerRadius: 5))
    }

    private var placeholder: some View {
        Image(systemName: leadingIcon)
            .font(.system(size: 15, weight: .medium))
            .foregroundStyle(isCurrent ? Color.green : Color.secondary)
    }

    private var leadingIcon: String {
        if isPlaying {
            return "speaker.wave.2.fill"
        }
        if hasArtworkHint {
            return "photo"
        }
        return "music.note"
    }
}

private struct PlaylistArtworkThumbnail: View {
    let artworkURL: URL?

    var body: some View {
        ZStack {
            #if os(macOS)
            if let artworkURL, let image = NSImage(contentsOf: artworkURL) {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: 22, height: 22)
                    .clipped()
            } else {
                Image(systemName: "music.note.house")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            #else
            Image(systemName: "music.note.house")
                .font(.caption)
                .foregroundStyle(.secondary)
            #endif
        }
        .frame(width: 22, height: 22)
        .background(Color(nsColor: .separatorColor).opacity(0.18))
        .clipShape(RoundedRectangle(cornerRadius: 4))
    }
}

private struct NowPlayingBackdrop: View {
    let artworkURL: URL?

    var body: some View {
        ZStack {
            Color(nsColor: .windowBackgroundColor)

            if let artworkURL, let image = NSImage(contentsOf: artworkURL) {
                GeometryReader { proxy in
                    Image(nsImage: image)
                        .resizable()
                        .scaledToFill()
                        .frame(width: proxy.size.width, height: proxy.size.height)
                        .clipped()
                        .blur(radius: 64)
                        .scaleEffect(1.12)
                        .opacity(0.24)
                }
            }

            Rectangle()
                .fill(.ultraThinMaterial)
            Color(nsColor: .windowBackgroundColor)
                .opacity(0.36)
        }
        .ignoresSafeArea()
        .clipped()
    }
}

private struct ArtworkViewport: View {
    let artworkURL: URL?
    let size: CGFloat

    var body: some View {
        ZStack {
            #if os(macOS)
            if let artworkURL, let image = NSImage(contentsOf: artworkURL) {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: size, height: size)
                    .clipped()
            } else {
                placeholder
            }
            #else
            placeholder
            #endif
        }
        .frame(width: size, height: size)
        .background(Color(nsColor: .separatorColor).opacity(0.22))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(Color(nsColor: .separatorColor).opacity(0.38), lineWidth: 1)
        )
    }

    private var placeholder: some View {
        VStack(spacing: 12) {
            Image(systemName: "music.note")
                .font(.system(size: 58, weight: .medium))
            Text("No Artwork")
                .font(.callout.weight(.medium))
        }
        .foregroundStyle(.secondary)
    }
}
#endif
