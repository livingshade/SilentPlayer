#if os(iOS)
import Foundation
import SwiftUI

struct PhonePlaybackQueueSheet: View {
    @ObservedObject var model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section("Playback") {
                    Picker(
                        "Playback Order",
                        selection: Binding(
                            get: { model.playback.playbackMode },
                            set: { mode in
                                Task { await model.setPlaybackMode(mode) }
                            }
                        )
                    ) {
                        ForEach(PlaybackMode.allCases) { mode in
                            Label(mode.label, systemImage: mode.systemImage)
                                .tag(mode)
                        }
                    }
                    .pickerStyle(.menu)
                }

                if model.playback.queue.isEmpty {
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
                    if model.playback.playbackMode == .shuffle {
                        Label("Showing the shuffled playback order", systemImage: "shuffle")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else if model.playback.playbackMode == .repeatOne {
                        Label("The current song repeats until the playback order changes", systemImage: "repeat.1")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }

                    ForEach(Array(model.playback.queue.enumerated()), id: \.element.id) { index, track in
                        Button {
                            Task { await model.playQueueItem(at: index) }
                        } label: {
                            HStack(spacing: 12) {
                                Image(systemName: model.playback.queuePosition == index ? "speaker.wave.2.fill" : "play.fill")
                                    .foregroundStyle(model.playback.queuePosition == index ? Color.accentColor : Color.secondary)
                                    .frame(width: 20)

                                VStack(alignment: .leading, spacing: 2) {
                                    Text(track.phoneDisplayTitle)
                                        .lineLimit(2)
                                    Text(track.phoneDisplaySubtitle)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                }

                                Spacer(minLength: 0)
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityElement(children: .combine)
                        .accessibilityLabel("Play \(track.phoneDisplayTitle)")
                        .accessibilityHint("Jumps to this song in the queue")
                        .accessibilityValue(model.playback.queuePosition == index ? "Currently selected" : "")
                        .moveDisabled(model.playback.playbackMode == .shuffle)
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
                        .disabled(model.playback.queue.isEmpty)

                    Button("Clear", role: .destructive) {
                        Task { await model.clearPlaybackQueue() }
                    }
                    .disabled(model.playback.queue.isEmpty)
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
        guard model.playback.queue.indices.contains(target) else {
            return
        }
        Task { await model.moveQueueItem(from: source, to: target) }
    }
}

struct PhoneAppAlert: Identifiable {
    let id = UUID()
    let title: String
    let message: String
}

#endif
