#if os(macOS)
import Foundation
import SwiftUI

extension ContentView {
    internal var trackList: some View {
        List(selection: Binding(
            get: { model.library.selectedTrack?.id },
            set: { id in
                model.selectTrack(id: id)
                persistPresentation()
            }
        )) {
            ForEach(model.library.tracks) { track in
                trackRow(for: track)
            }
        }
        .overlay {
            if model.library.tracks.isEmpty {
                VStack(spacing: 10) {
                    Image(systemName: emptyIcon)
                        .font(.system(size: 42))
                        .foregroundStyle(.secondary)
                    Text(model.library.scope.title)
                        .font(.title3.weight(.semibold))
                    Text(model.operations.status)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    internal var playerBar: some View {
        VStack(spacing: 9) {
            HStack(spacing: 12) {
                Button {
                    guard model.playback.nowPlaying != nil else {
                        return
                    }
                    toggleExpandedNowPlaying()
                } label: {
                    HStack(spacing: 12) {
                        if let track = model.playback.nowPlaying {
                            TrackArtworkThumbnail(
                                artworkURL: track.artworkURL,
                                isCurrent: true,
                                isPlaying: model.playback.isPlaying,
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
                            Text(model.playback.nowPlaying?.title ?? "Nothing playing")
                                .font(.headline)
                                .lineLimit(1)
                                .truncationMode(.tail)
                            Text(model.playback.nowPlaying?.subtitle ?? "Choose a song to start listening")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .truncationMode(.tail)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)

                        if model.playback.nowPlaying != nil {
                            Image(systemName: isNowPlayingExpanded ? "chevron.down" : "chevron.up")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(model.playback.nowPlaying == nil)
                .frame(width: 240, alignment: .leading)
                .help(isNowPlayingExpanded ? "Close Now Playing" : "Open Now Playing")

                Spacer(minLength: 12)

                HStack(spacing: 14) {
                    Button {
                        Task { await model.toggleShuffle() }
                    } label: {
                        Label("Shuffle", systemImage: "shuffle")
                            .labelStyle(.iconOnly)
                            .font(.title3)
                            .foregroundStyle(model.playback.isShuffleEnabled ? Color.accentColor : Color.secondary)
                            .frame(width: 30, height: 30)
                    }
                    .buttonStyle(.borderless)
                    .help(model.playback.isShuffleEnabled ? "Shuffle on" : "Shuffle off")

                    Button {
                        Task { await model.previousTrack() }
                    } label: {
                        Label("Previous", systemImage: "backward.fill")
                            .labelStyle(.iconOnly)
                            .font(.title2)
                            .frame(width: 34, height: 34)
                    }
                    .buttonStyle(.borderless)
                    .help("Previous")

                    Button {
                        Task { await model.pauseOrResume() }
                    } label: {
                        Label(model.playback.isPlaying ? "Pause" : "Play", systemImage: model.playback.isPlaying ? "pause.fill" : "play.fill")
                            .labelStyle(.iconOnly)
                            .font(.title2)
                            .frame(width: 36, height: 36)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .clipShape(Circle())
                    .keyboardShortcut(.space, modifiers: [])
                    .help(model.playback.isPlaying ? "Pause" : "Play")

                    Button {
                        Task { await model.nextTrack() }
                    } label: {
                        Label("Next", systemImage: "forward.fill")
                            .labelStyle(.iconOnly)
                            .font(.title2)
                            .frame(width: 34, height: 34)
                    }
                    .buttonStyle(.borderless)
                    .help("Next")

                    Menu {
                        ForEach(PlaybackRepeatMode.allCases) { mode in
                            Button {
                                Task { await model.setRepeatMode(mode) }
                            } label: {
                                Label(mode.label, systemImage: model.playback.repeatMode == mode ? "checkmark" : mode.systemImage)
                            }
                        }
                    } label: {
                        Label(model.playback.repeatMode.label, systemImage: model.playback.repeatMode.systemImage)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(model.playback.repeatMode == .off ? Color.secondary : Color.accentColor)
                            .lineLimit(1)
                            .frame(maxWidth: .infinity)
                            .frame(height: 30)
                    }
                    .menuStyle(.button)
                    .controlSize(.regular)
                    .frame(width: 118)
                    .help("Repeat mode")
                }

                Button {
                    isQueuePresented = true
                } label: {
                    Label(model.queueStatusText, systemImage: "music.note.list")
                        .font(.callout.weight(.semibold))
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(width: 120)
                }
                .buttonStyle(.bordered)
                .controlSize(.regular)
                .help("Show queue")

                if model.operations.isBusy {
                    ProgressView()
                        .controlSize(.small)
                }
            }

            HStack(spacing: 10) {
                Text(seekTimeText)
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(width: 92, alignment: .leading)

                Slider(
                    value: seekBinding,
                    in: 0...1,
                    onEditingChanged: handleSeekEditingChanged
                )
                .controlSize(.large)
                .frame(height: 24)
                .disabled(model.playback.nowPlaying?.durationMS == nil)
            }

            if !model.playback.error.isEmpty {
                HStack {
                    Label(model.playback.error, systemImage: "exclamationmark.triangle.fill")
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
}
#endif
