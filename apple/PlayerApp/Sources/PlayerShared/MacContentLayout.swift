#if os(macOS)
import Foundation
import SwiftUI

private struct PlaylistSearchRequest: Equatable {
    let query: String
    let playlists: [PlaylistItem]
}

extension ContentView {
    internal var playerContent: some View {
        NavigationSplitView(columnVisibility: $splitViewVisibility) {
            sidebar
                .navigationSplitViewColumnWidth(min: 220, ideal: 270, max: 340)
        } detail: {
            detailPane
        }
        .searchable(text: $playlistSearchText, placement: .sidebar, prompt: "Search playlists")
    }

    @MainActor
    internal func restorePresentation() async {
        let requestedSnapshot = MacPresentationPersistence.decode(sceneSession) ?? .initial
        await model.bootstrap(
            restoring: requestedSnapshot.contentScope,
            preferredSelectedTrackID: requestedSnapshot.selectedTrackID
        )
        isRestoringPresentation = false
        persistPresentation()
    }

    internal func persistPresentation() {
        guard !isRestoringPresentation else {
            return
        }
        let snapshot = MacPresentationSnapshot(
            contentScope: model.restorableLibraryScope,
            selectedTrackID: model.library.selectedTrack?.id
        )
        guard let encoded = MacPresentationPersistence.encode(snapshot) else {
            return
        }
        sceneSession = encoded
    }

    internal func toggleExpandedNowPlaying() {
        if isNowPlayingExpanded {
            dismissExpandedNowPlaying()
        } else {
            presentExpandedNowPlaying()
        }
    }

    internal func presentExpandedNowPlaying() {
        guard model.playback.nowPlaying != nil else {
            return
        }
        isNowPlayingExpanded = true
    }

    internal func dismissExpandedNowPlaying() {
        guard isNowPlayingExpanded else {
            return
        }
        isNowPlayingExpanded = false
    }

    internal var restorationPlaceholder: some View {
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

    internal var detailPane: some View {
        ZStack {
            VStack(spacing: 0) {
                if isNowPlayingExpanded, let track = model.playback.nowPlaying {
                    expandedNowPlaying(for: track)
                        .layoutPriority(1)
                    Divider()
                    playerBar
                        .fixedSize(horizontal: false, vertical: true)
                } else {
                    contentHeader
                    Divider()
                    trackList
                        .layoutPriority(1)
                    Divider()
                    playerBar
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            if model.operations.isBusy {
                busyOverlay
            }
        }
    }

    internal var busyOverlay: some View {
        ZStack {
            Color.black.opacity(0.12)
                .ignoresSafeArea()
            VStack(spacing: 10) {
                ProgressView()
                    .controlSize(.large)
                Text(model.operations.status)
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

    internal var sidebar: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Silent")
                    .font(.title2.weight(.semibold))
                Text(model.library.scope.title)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            VStack(spacing: 6) {
                scopeButton("Library", icon: "music.note.list", selected: model.library.scope == .library) {
                    await model.showLibrary()
                }
                scopeButton("History", icon: "clock.arrow.circlepath", selected: model.library.scope == .history) {
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
                    sidebarPlaylistRows
                }
                .frame(maxWidth: .infinity)
            }
            .task(id: PlaylistSearchRequest(query: playlistSearchQuery, playlists: model.playlists.items)) {
                await searchSidebarPlaylists(playlistSearchQuery)
            }

            Spacer()

            VStack(alignment: .leading, spacing: 5) {
                Text(model.operations.status)
                    .font(.callout)
                    .foregroundStyle(model.operations.isBusy ? Color.orange : Color.secondary)
                    .lineLimit(2)
                if model.operations.isLibraryWorking || !model.operations.libraryStatus.isEmpty {
                    libraryProgress
                }
                if model.operations.isAnalyzing || !model.operations.analyzeStatus.isEmpty {
                    analyzerProgress
                }
            }
        }
        .padding()
        .frame(minWidth: 220, idealWidth: 270, maxWidth: 340)
    }

    private var playlistSearchQuery: String {
        playlistSearchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    @ViewBuilder
    private var sidebarPlaylistRows: some View {
        if playlistSearchQuery.isEmpty {
            ForEach(model.playlists.items, content: playlistButton)
        } else if let search = playlistSearchResult, search.query == playlistSearchQuery {
            switch search.result {
            case .success(let matches) where !matches.isEmpty:
                ForEach(matches, content: playlistButton)
            case .success:
                Text("No matching playlists")
                    .foregroundStyle(.secondary)
            case .failure:
                Text("Couldn’t search playlists")
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func searchSidebarPlaylists(_ query: String) async {
        guard !query.isEmpty else {
            playlistSearchResult = nil
            return
        }
        do {
            try await Task.sleep(for: .milliseconds(120))
            let matches = try await model.invoke { try $0.searchPlaylists(query) }
            try Task.checkCancellation()
            playlistSearchResult = (query, .success(matches))
        } catch is CancellationError {
            return
        } catch {
            guard !Task.isCancelled else { return }
            playlistSearchResult = (query, .failure(error))
        }
    }

    internal var contentHeader: some View {
        Group {
            if let playlist = activePlaylist {
                playlistHeader(for: playlist)
            } else {
                collectionHeader
            }
        }
    }

    private var collectionHeader: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 2) {
                Text(model.library.scope.title)
                    .font(.title3.weight(.semibold))
                    .lineLimit(1)
                Text("\(model.library.tracks.count) songs")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 12)

            if supportsSongSearch {
                songSearchField
                    .frame(width: 280)
                    .background(Color(nsColor: .controlBackgroundColor))
                    .clipShape(RoundedRectangle(cornerRadius: 7))
                    .overlay {
                        RoundedRectangle(cornerRadius: 7)
                            .stroke(Color(nsColor: .separatorColor).opacity(0.55), lineWidth: 1)
                    }
            }

            if supportsCollectionPlayAll {
                collectionPlayAllButton
            }

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

    private func playlistHeader(for playlist: PlaylistItem) -> some View {
        let accent = Color(red: 0.85, green: 0.10, blue: 0.29)
        let createdAt = Date(timeIntervalSince1970: TimeInterval(playlist.createdAtUnixSeconds))
        return VStack(alignment: .leading, spacing: 20) {
            HStack(alignment: .top, spacing: 24) {
                PlaylistArtworkThumbnail(artworkURL: playlist.artworkURL, size: 176)
                    .accessibilityHidden(true)

                VStack(alignment: .leading, spacing: 12) {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text(playlist.name)
                            .font(.largeTitle.weight(.semibold))
                            .lineLimit(2)
                            .accessibilityAddTraits(.isHeader)

                        Button {
                            model.presentPlaylistSettings(playlist)
                        } label: {
                            Label("Edit Playlist", systemImage: "square.and.pencil")
                                .labelStyle(.iconOnly)
                                .font(.title3)
                        }
                        .buttonStyle(.borderless)
                        .help("Edit playlist")
                    }

                    if playlist.createdAtUnixSeconds > 0 {
                        Text("Created \(createdAt.formatted(date: .abbreviated, time: .omitted))")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }

                    Spacer(minLength: 0)

                    HStack(spacing: 10) {
                        collectionPlayAllButton
                            .controlSize(.large)
                            .tint(accent)

                        Menu {
                            contentActionsMenu
                        } label: {
                            Label("More", systemImage: "ellipsis")
                                .labelStyle(.iconOnly)
                        }
                        .menuIndicator(.hidden)
                        .menuStyle(.borderedButton)
                        .controlSize(.large)
                        .help("More actions")
                    }
                }
                .frame(maxWidth: .infinity, alignment: .topLeading)
                .frame(height: 176, alignment: .topLeading)
            }

            HStack {
                Text(playlist.trackCount == 1 ? "1 song" : "\(playlist.trackCount) songs")
                    .font(.headline)
                    .padding(.bottom, 5)
                    .overlay(alignment: .bottomLeading) {
                        Capsule()
                            .fill(accent)
                            .frame(width: 18, height: 3)
                            .accessibilityHidden(true)
                    }

                Spacer()

                songSearchField
                    .frame(minWidth: 140, idealWidth: 190, maxWidth: 230)
                    .background(.white.opacity(0.12), in: Capsule())
            }
        }
        .fixedSize(horizontal: false, vertical: true)
        .padding(.horizontal, 22)
        .padding(.vertical, 16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .foregroundStyle(.white)
        .environment(\.colorScheme, .dark)
        .background {
            LinearGradient(
                colors: [Color(red: 0.35, green: 0.19, blue: 0.38), Color(red: 0.19, green: 0.12, blue: 0.21)],
                startPoint: .top,
                endPoint: .bottom
            )
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("playlistSummary")
    }

    private var songSearchField: some View {
        HStack(spacing: 7) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)

            TextField("Search songs", text: featureBinding(model.library, \.query))
                .textFieldStyle(.plain)
                .onSubmit {
                    Task { await model.search() }
                }

            if !model.library.query.isEmpty {
                Button {
                    clearSearch()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.borderless)
                .accessibilityLabel("Clear song search")
                .help("Clear search")
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
    }

    private var collectionPlayAllButton: some View {
        Button {
            Task { await playAllCurrentCollection() }
        } label: {
            Label("Play All", systemImage: "play.fill")
                .font(.callout.weight(.semibold))
        }
        .buttonStyle(.borderedProminent)
        .disabled(!canPlayAllCurrentCollection || model.operations.isBusy)
        .help("Replace the queue with all songs in \(model.library.scope.title) and start playing")
    }

    @ViewBuilder
    internal var contentActionsMenu: some View {
        Button {
            if model.operations.isLibraryWorking {
                model.stopLibraryWork()
            } else {
                Task { await model.reloadActiveScope() }
            }
        } label: {
            Label(
                model.operations.isLibraryWorking ? "Stop Library Task" : "Refresh",
                systemImage: model.operations.isLibraryWorking ? "stop.circle" : "arrow.clockwise"
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
        .disabled(model.operations.isLibraryWorking)

        Button {
            if model.operations.isAnalyzing {
                model.stopAnalyze()
            } else {
                Task { await model.analyzeLibrary() }
            }
        } label: {
            Label(
                model.operations.isAnalyzing ? "Stop Analysis" : "Analyze Library",
                systemImage: model.operations.isAnalyzing ? "stop.circle" : "waveform"
            )
        }

        Button {
            Task { await model.auditDatabase() }
        } label: {
            Label("Audit Library", systemImage: "checklist.checked")
        }
        .disabled(model.operations.isLibraryWorking)

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
        .disabled(model.operations.isBusy || model.operations.isLibraryWorking || model.operations.isAnalyzing)

        Divider()

        Button {
            isLibraryInformationPresented = true
        } label: {
            Label("Library Information…", systemImage: "info.circle")
        }
    }

    internal var activePlaylistMenu: some View {
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
            .disabled(model.library.selectedTrack == nil)

            Button {
                Task { await model.moveSelectedInActivePlaylist(delta: 1) }
            } label: {
                Label("Move Selected Down", systemImage: "arrow.down")
            }
            .disabled(model.library.selectedTrack == nil)

            Button {
                Task { await model.removeSelectedFromActivePlaylist() }
            } label: {
                Label("Remove Selected", systemImage: "minus.circle")
            }
            .disabled(model.library.selectedTrack == nil)

            Divider()

            Button(role: .destructive) {
                pendingPlaylistSongRemoval = activePlaylist
                isPlaylistSongRemovalConfirmationPresented = true
            } label: {
                Label("Remove All from Playlist…", systemImage: "minus.circle")
            }
            .disabled(!canRemovePlaylistSongs)

            Button(role: .destructive) {
                pendingPlaylistLibraryDeletion = activePlaylist
                isPlaylistLibraryDeletionConfirmationPresented = true
            } label: {
                Label("Delete All from Library…", systemImage: "trash")
            }
            .disabled(!canRemovePlaylistSongs)

            Divider()

            Button(role: .destructive) {
                Task { await model.deleteActivePlaylist() }
            } label: {
                Label("Delete Playlist", systemImage: "trash")
            }
        } label: {
            Label("Playlist", systemImage: "music.note.list")
        }
    }

    internal var activePlaylist: PlaylistItem? {
        guard let name = model.activePlaylistName else {
            return nil
        }
        return model.playlists.items.first { $0.name == name }
    }

    internal var canRemovePlaylistSongs: Bool {
        (activePlaylist?.trackCount ?? 0) > 0
            && !model.operations.isBusy
            && !model.operations.isLibraryWorking
            && !model.operations.isAnalyzing
    }

    internal func clearSearch() {
        model.library.query = ""
        Task { await model.reloadActiveScope() }
    }

    internal var supportsSongSearch: Bool {
        switch model.library.scope {
        case .library, .playlist:
            return true
        case .favorites, .history:
            return false
        }
    }

    internal var supportsCollectionPlayAll: Bool {
        switch model.library.scope {
        case .library, .playlist:
            return true
        case .favorites, .history:
            return false
        }
    }

    internal var canPlayAllCurrentCollection: Bool {
        switch model.library.scope {
        case .library:
            return !model.library.tracks.isEmpty || !model.library.query.isEmpty
        case .playlist:
            return (activePlaylist?.trackCount ?? 0) > 0
        case .favorites, .history:
            return false
        }
    }

    @MainActor
    internal func playAllCurrentCollection() async {
        switch model.library.scope {
        case .library:
            await model.playEntireLibrary()
        case .playlist:
            guard let playlist = activePlaylist else {
                return
            }
            await model.playPlaylist(playlist, shuffled: false)
        case .favorites, .history:
            return
        }
    }
}
#endif
