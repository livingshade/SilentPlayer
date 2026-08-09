#if os(macOS)
import Foundation
import SwiftUI

struct PlaybackQueueSheet: View {
    @ObservedObject var model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                if model.playback.queue.isEmpty {
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
                    ForEach(Array(model.playback.queue.enumerated()), id: \.element.id) { index, track in
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
                    .disabled(model.playback.queue.isEmpty)
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
            Image(systemName: model.playback.queuePosition == index ? "speaker.wave.2.fill" : "line.3.horizontal")
                .foregroundStyle(model.playback.queuePosition == index ? Color.accentColor : Color.secondary)
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
            .disabled(index + 1 >= model.playback.queue.count)
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
        guard model.playback.queue.indices.contains(target) else {
            return
        }
        Task { await model.moveQueueItem(from: source, to: target) }
    }
}
#endif
