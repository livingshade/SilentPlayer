#if os(macOS)
import AppKit
import Foundation
import SwiftUI

public struct ContentView: View {
    @ObservedObject private var model: AppModel
    @State private var pendingSeekProgress: Double?
    @State private var pendingSingleClick: DispatchWorkItem?
    @State private var isViewChecksExpanded = false
    @State private var isZeroOutConfirmationPresented = false
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
        NavigationSplitView {
            sidebar
                .navigationSplitViewColumnWidth(min: 220, ideal: 270, max: 340)
        } detail: {
            GeometryReader { proxy in
                detailPane(layout: DetailPaneLayout(containerSize: proxy.size))
            }
        }
        .frame(minWidth: 960, idealWidth: 1180, minHeight: 620, idealHeight: 780)
        .sheet(isPresented: $model.isViewEditPresented) {
            TrackViewEditSheet(
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
        .confirmationDialog(
            "Zero Out Library?",
            isPresented: $isZeroOutConfirmationPresented,
            titleVisibility: .visible
        ) {
            Button("Back Up and Zero Out", role: .destructive) {
                Task { await model.zeroOutLibrary() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Silent will export a complete backup before clearing the database and managed music files.")
        }
        .task {
            await model.bootstrap()
        }
    }

    private func detailPane(layout: DetailPaneLayout) -> some View {
        ZStack {
            VStack(spacing: 0) {
                toolbar
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
                    await model.refreshLibrary()
                }
                scopeButton("Favorites", icon: "heart.fill", selected: model.libraryScope == .favorites) {
                    await model.showFavorites()
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
                Text("DB: \(model.dbPath)")
                    .font(.caption2.monospaced())
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text("Music: \(model.mediaRootPath)")
                    .font(.caption2.monospaced())
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
        .padding()
        .frame(minWidth: 220, idealWidth: 270, maxWidth: 340)
    }

    private var toolbar: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            TextField("Search title, artist, album, or path", text: $model.query)
                .textFieldStyle(.roundedBorder)
                .onSubmit {
                    Task { await model.search() }
                }

            Button {
                Task { await model.search() }
            } label: {
                Label("Search", systemImage: "magnifyingglass")
            }
            .help("Search")

            Button {
                model.query = ""
                Task { await model.reloadActiveScope() }
            } label: {
                Label("Clear", systemImage: "xmark.circle")
            }
            .help("Clear search")

            Divider()

            Button {
                Task {
                    if let folder = await chooseFolder() {
                        await model.importFolder(folder)
                    }
                }
            } label: {
                Label("Import Music", systemImage: "folder.badge.plus")
            }
            .disabled(model.isLibraryWorking)

            Menu {
                Button {
                    Task {
                        if let packageURL = await chooseLibraryExportPackage() {
                            await model.exportLibrary(to: packageURL)
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
                Label("Library", systemImage: "externaldrive")
            }
            .help("Export, replace, or clear the complete library")
            .disabled(model.isBusy || model.isLibraryWorking || model.isAnalyzing)

            Button {
                if model.isLibraryWorking {
                    model.stopLibraryWork()
                } else {
                    Task { await model.reloadActiveScope() }
                }
            } label: {
                Label(model.isLibraryWorking ? "Stop" : "Refresh", systemImage: model.isLibraryWorking ? "stop.circle" : "arrow.clockwise")
            }
            .help(model.isLibraryWorking ? "Stop library task" : "Refresh")

            Button {
                Task { await model.auditDatabase() }
            } label: {
                Label("Audit Library", systemImage: "checklist.checked")
            }
            .disabled(model.isLibraryWorking)

            Divider()

            Menu {
                playlistSortButton(.defaultOrder)
                Divider()
                playlistSortButton(.title)
                playlistSortButton(.artist)
                playlistSortButton(.album)
                playlistSortButton(.rating)
            } label: {
                Label("Sort", systemImage: "arrow.up.arrow.down")
            }
            .help("Sort tracks")

            if model.activePlaylistName != nil {
                playlistActionsMenu
            }

            if model.isAnalyzing {
                Button {
                    model.stopAnalyze()
                } label: {
                    Label("Stop Analyze", systemImage: "stop.circle")
                }
            } else {
                Button {
                    Task { await model.analyzeLibrary() }
                } label: {
                    Label("Analyze", systemImage: "waveform")
                }
            }
        }
        .padding()
    }

    private var trackList: some View {
        List(selection: Binding(
            get: { model.selectedTrack?.id },
            set: { id in model.selectTrack(id: id) }
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
        VStack(spacing: 8) {
            HStack(spacing: 14) {
                Button {
                    Task { await model.toggleShuffle() }
                } label: {
                    Label("Shuffle", systemImage: "shuffle")
                        .foregroundStyle(model.isShuffleEnabled ? Color.accentColor : Color.secondary)
                }
                .buttonStyle(.borderless)
                .help(model.isShuffleEnabled ? "Shuffle on" : "Shuffle off")

                HStack(spacing: 10) {
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
                        Task { await model.stopPlayback() }
                    } label: {
                        Label("Stop", systemImage: "stop.fill")
                            .labelStyle(.iconOnly)
                    }
                    .help("Stop")

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

                VStack(alignment: .leading, spacing: 3) {
                    Text(model.nowPlaying?.title ?? "Nothing playing")
                        .font(.headline)
                        .lineLimit(1)
                    Text(model.nowPlaying?.subtitle ?? model.status)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }

                Spacer()

                Button {
                    Task { await model.setSelectedFavorite(true) }
                } label: {
                    Label("Favorite", systemImage: "heart")
                }
                .buttonStyle(.borderless)
                .help("Favorite")

                Button {
                    Task { await model.addSelectedToPlaylist() }
                } label: {
                    Label("Playlist", systemImage: "text.badge.plus")
                }
                .buttonStyle(.borderless)
                .help("Add to playlist")

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

                Text(model.normalizeText)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .frame(width: 128, alignment: .trailing)
                    .lineLimit(1)

                Text(model.queueStatusText)
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(width: 96, alignment: .trailing)
                    .lineLimit(1)
            }

            HStack {
                Text(model.playbackError.isEmpty ? model.playbackDetail : model.playbackError)
                    .font(.caption2)
                    .foregroundStyle(model.playbackError.isEmpty ? Color.secondary : Color.red)
                    .lineLimit(2)
                    .textSelection(.enabled)
                Spacer()
            }
            .frame(minHeight: 16)
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
                            viewPicker
                                .frame(maxWidth: 220, alignment: .trailing)
                            HStack(spacing: 8) {
                                artworkMenu(for: track)

                                Button {
                                    model.presentViewEdit()
                                } label: {
                                    Label("Edit View", systemImage: "pencil")
                                }
                                .disabled(model.isLoadingDetails || model.detailTrack == nil)

                                Button {
                                    materialize(track)
                                } label: {
                                    Label("Export View", systemImage: "square.and.arrow.down")
                                }
                                .disabled(model.detailTrack == nil)
                            }
                        }
                    }

                    secondaryContentPanels
                    advancedViewPanel
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

    private var viewPicker: some View {
        let choices = model.detailViewChoices
        return Group {
            if choices.count > 1 {
                Picker(
                    "View",
                    selection: Binding(
                        get: { model.detailTrack?.id ?? "" },
                        set: { model.selectDetailView(id: $0) }
                    )
                ) {
                    ForEach(choices) { choice in
                        Text(viewChoiceTitle(choice))
                            .tag(choice.id)
                    }
                }
                .pickerStyle(.menu)
            }
        }
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

    private func artworkMenu(for track: TrackItem) -> some View {
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
        } label: {
            Label("Cover", systemImage: "photo.on.rectangle")
        }
        .help("Cover artwork")
    }

    private func viewChoiceTitle(_ choice: TrackViewChoice) -> String {
        var title = choice.track.viewName ?? (choice.track.isPrimaryView ? "Primary view" : "View \(choice.index + 1)")
        var details: [String] = []
        if let format = choice.track.formatName?.trimmingCharacters(in: .whitespacesAndNewlines),
           !format.isEmpty {
            details.append(format.uppercased())
        }
        if let quality = choice.track.qualityProfile?.trimmingCharacters(in: .whitespacesAndNewlines),
           !quality.isEmpty {
            details.append(quality)
        }
        if !details.isEmpty {
            title += " - " + details.joined(separator: " / ")
        }
        return title
    }

    @ViewBuilder
    private var secondaryContentPanels: some View {
        let hasLyrics = model.nowPlayingDetails?.hasLyrics ?? false
        let notes = model.nowPlayingDetails?.notes?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if hasLyrics {
            lyricsPanel
        }
        if !notes.isEmpty {
            notesPanel
        }
    }

    private var advancedViewPanel: some View {
        Group {
            if let details = model.nowPlayingDetails {
                let errorDiagnostics = details.diagnostics.filter { $0.severity == .error }
                let optionalDiagnostics = details.diagnostics.filter { $0.severity != .error }

                VStack(alignment: .leading, spacing: 8) {
                    if !errorDiagnostics.isEmpty {
                        diagnosticsList(errorDiagnostics)
                    }

                    DisclosureGroup(isExpanded: $isViewChecksExpanded) {
                        VStack(alignment: .leading, spacing: 8) {
                            Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 5) {
                                viewFieldRow("View name", optionalViewValue(details.viewName))
                                viewFieldRow("View ID", details.viewID)
                                viewFieldRow("Primary", details.primaryViewID)
                                viewFieldRow("Kind", details.viewKind)
                                viewFieldRow("Format", optionalViewValue(details.formatName))
                                viewFieldRow("Quality", optionalViewValue(details.qualityProfile))
                                viewFieldRow("Artwork", optionalViewValue(details.artworkSource))
                                viewFieldRow("Transform", optionalViewValue(details.transformSpec))
                            }
                            .font(.caption)

                            if !optionalDiagnostics.isEmpty {
                                diagnosticsList(optionalDiagnostics)
                            }
                        }
                        .padding(.top, 4)
                    } label: {
                        Label(
                            details.isPrimaryView ? "Primary View Details" : "Derived View Details",
                            systemImage: details.isPrimaryView ? "circle.fill" : "square.on.circle"
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

    private func diagnosticsList(_ diagnostics: [TrackViewDiagnostic]) -> some View {
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

    private func viewFieldRow(_ label: String, _ value: String) -> some View {
        GridRow {
            Text(label)
                .foregroundStyle(.secondary)
            Text(value)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
        }
    }

    private func optionalViewValue(_ value: String?) -> String {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines), !value.isEmpty else {
            return "Not set"
        }
        return value
    }

    private func diagnosticIcon(_ severity: TrackViewDiagnosticSeverity) -> String {
        switch severity {
        case .error:
            return "xmark.octagon.fill"
        case .warning:
            return "exclamationmark.triangle.fill"
        case .info:
            return "info.circle"
        }
    }

    private func diagnosticColor(_ severity: TrackViewDiagnosticSeverity) -> Color {
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
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Text("Lyrics")
                    .font(.headline)
                Spacer()
                if let lyricsURL = model.nowPlayingDetails?.lyricsURL {
                    Text(lyricsURL.lastPathComponent)
                        .font(.caption2.monospaced())
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }

            if let lyricsText = model.nowPlayingDetails?.lyricsText,
               !lyricsText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                ScrollView {
                    Text(lyricsText)
                        .font(.callout)
                        .foregroundStyle(.primary)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                }
                .frame(maxHeight: 154)
                .background(Color(nsColor: .textBackgroundColor))
                .clipShape(RoundedRectangle(cornerRadius: 6))
            } else {
                Text("No lyrics file")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, minHeight: 74, alignment: .center)
                    .background(Color(nsColor: .textBackgroundColor))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
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

    private var playlistActionsMenu: some View {
        Menu {
            Button {
                Task { await model.moveSelectedInActivePlaylist(delta: -1) }
            } label: {
                Label("Move Up", systemImage: "arrow.up")
            }
            .disabled(model.selectedTrack == nil)

            Button {
                Task { await model.moveSelectedInActivePlaylist(delta: 1) }
            } label: {
                Label("Move Down", systemImage: "arrow.down")
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
            Label("Playlist", systemImage: "ellipsis.circle")
        }
        .help("Playlist actions")
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
        }
        pendingSingleClick = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.18, execute: work)
    }

    private func selectTrackImmediately(_ track: TrackItem) {
        pendingSingleClick?.cancel()
        pendingSingleClick = nil
        model.selectTrack(id: track.id)
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
            selectTrackImmediately(track)
            Task { await model.setSelectedFavorite(true) }
        } label: {
            Label("Favorite", systemImage: "heart")
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
            model.presentViewEdit()
        } label: {
            Label("Edit View", systemImage: "pencil")
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
            Label("Export View", systemImage: "square.and.arrow.down")
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

            Button {
                selectTrackImmediately(track)
                Task { await model.removeSelectedFromActivePlaylist() }
            } label: {
                Label("Remove from Playlist", systemImage: "minus.circle")
            }
        }
    }

    private func scopeButton(
        _ title: String,
        icon: String,
        selected: Bool,
        action: @escaping () async -> Void
    ) -> some View {
        Button {
            Task { await action() }
        } label: {
            HStack {
                Label(title, systemImage: icon)
                Spacer()
            }
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(selected ? Color.accentColor.opacity(0.14) : Color.clear)
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    private func playlistButton(_ playlist: PlaylistItem) -> some View {
        let selected = model.libraryScope == .playlist(playlist.name)
        return Button {
            Task { await model.showPlaylist(playlist) }
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
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(selected ? Color.accentColor.opacity(0.14) : Color.clear)
        .clipShape(RoundedRectangle(cornerRadius: 6))
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

private struct TrackViewEditSheet: View {
    @ObservedObject var model: AppModel
    let chooseArtworkFile: () async -> URL?
    let chooseLyricsFile: () async -> URL?

    var body: some View {
        NavigationStack {
            Form {
                Section("View") {
                    TextField("Name", text: $model.viewEditNameDraft)
                    readOnlyRow("Kind", isPrimaryView ? "Primary" : "Derived")
                    readOnlyRow("Format", formatName)
                }

                Section("Music") {
                    TextField("Title", text: $model.viewEditTitleDraft)
                    TextField("Artist", text: $model.viewEditArtistDraft)
                    TextField("Album", text: $model.viewEditAlbumDraft)
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
                                        model.setViewEditArtworkURL(url)
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
                                        model.setViewEditLyricsURL(url)
                                    }
                                }
                            }
                        } label: {
                            Label("Choose", systemImage: "folder")
                        }
                    }
                }

                Section("Notes") {
                    TextEditor(text: $model.viewEditNotesDraft)
                        .font(.callout)
                        .frame(minHeight: 120)
                }
            }
            .navigationTitle("Edit View")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.cancelViewEdit()
                    }
                    .disabled(model.isViewSaving)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        Task { await model.saveViewEdit() }
                    }
                    .disabled(!canSave)
                }
            }
        }
        .frame(minWidth: 520, idealWidth: 560, maxWidth: 720, minHeight: 560, idealHeight: 620, maxHeight: 760)
        .interactiveDismissDisabled(model.isViewSaving)
    }

    private var canSave: Bool {
        !model.isViewSaving
            && model.viewEditChanged
            && !model.viewEditTitleDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var selectedArtworkName: String {
        model.viewEditArtworkURL?.lastPathComponent
            ?? model.nowPlayingDetails?.artworkURL?.lastPathComponent
            ?? "No Artwork"
    }

    private var selectedLyricsName: String {
        model.viewEditLyricsURL?.lastPathComponent
            ?? model.nowPlayingDetails?.lyricsURL?.lastPathComponent
            ?? "No Lyrics"
    }

    private var isPrimaryView: Bool {
        model.nowPlayingDetails?.isPrimaryView ?? model.detailTrack?.isPrimaryView ?? false
    }

    private var formatName: String {
        model.nowPlayingDetails?.formatName?.uppercased()
            ?? model.detailTrack?.formatName?.uppercased()
            ?? "Unknown"
    }

    private func readOnlyRow(_ title: String, _ value: String) -> some View {
        HStack {
            Text(title)
                .foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .lineLimit(1)
        }
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
