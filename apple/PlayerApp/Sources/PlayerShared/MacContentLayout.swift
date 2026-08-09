#if os(macOS)
import Foundation
import SwiftUI

extension ContentView {
    internal var playerContent: some View {
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

    internal func detailPane(layout: DetailPaneLayout) -> some View {
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

    internal struct DetailPaneLayout {
        let containerSize: CGSize

        var detailPanelHeight: CGFloat {
            let adaptiveHeight = containerSize.height * 0.34
            return min(max(adaptiveHeight, 220), 360)
        }

        var artworkSize: CGFloat {
            min(max(detailPanelHeight - 76, 144), 220)
        }
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
                    ForEach(model.playlists.items) { playlist in
                        playlistButton(playlist)
                    }
                }
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

    internal var contentHeader: some View {
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
                HStack(spacing: 7) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(.secondary)

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

            if supportsCollectionPlayAll {
                Button {
                    Task { await playAllCurrentCollection() }
                } label: {
                    Label("Play All", systemImage: "play.fill")
                        .font(.callout.weight(.semibold))
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.regular)
                .disabled(!canPlayAllCurrentCollection || model.operations.isBusy)
                .help("Replace the queue with all songs in \(model.library.scope.title) and start playing")
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

    internal var activePlaylist: PlaylistItem? {
        guard let name = model.activePlaylistName else {
            return nil
        }
        return model.playlists.items.first { $0.name == name }
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
