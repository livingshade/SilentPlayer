#if os(macOS)
import Foundation
import SwiftUI

extension ContentView {
    @ViewBuilder
    internal var secondaryContentPanels: some View {
        let notes = model.detailDetails?.notes?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        lyricsPanel
        if !notes.isEmpty {
            notesPanel
        }
    }

    internal var fileDetailsPanel: some View {
        Group {
            if let details = model.trackDetail.details {
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

    internal func diagnosticsList(_ diagnostics: [TrackDiagnostic]) -> some View {
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

    internal func fileFieldRow(_ label: String, _ value: String) -> some View {
        GridRow {
            Text(label)
                .foregroundStyle(.secondary)
            Text(value)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
        }
    }

    internal func optionalFileValue(_ value: String?) -> String {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines), !value.isEmpty else {
            return "Not set"
        }
        return value
    }

    internal func diagnosticIcon(_ severity: TrackDiagnosticSeverity) -> String {
        switch severity {
        case .error:
            return "xmark.octagon.fill"
        case .warning:
            return "exclamationmark.triangle.fill"
        case .info:
            return "info.circle"
        }
    }

    internal func diagnosticColor(_ severity: TrackDiagnosticSeverity) -> Color {
        switch severity {
        case .error:
            return .red
        case .warning:
            return .orange
        case .info:
            return .secondary
        }
    }

    internal var lyricsPanel: some View {
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
                isLoading: model.trackDetail.isLoading
            )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    internal var notesPanel: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Notes")
                .font(.headline)

            if let notes = model.trackDetail.details?.notes,
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

    internal var analyzerProgress: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let progress = model.operations.analyzeProgress {
                ProgressView(value: progress)
                    .controlSize(.small)
            } else if model.operations.isAnalyzing {
                ProgressView()
                    .controlSize(.small)
            }
            Text(model.operations.analyzeStatus)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
    }

    internal var libraryProgress: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let progress = model.operations.libraryProgress {
                ProgressView(value: progress)
                    .controlSize(.small)
            } else if model.operations.isLibraryWorking {
                ProgressView()
                    .controlSize(.small)
            }
            Text(model.operations.libraryStatus)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
    }

    internal var seekBinding: Binding<Double> {
        Binding(
            get: { pendingSeekProgress ?? model.playbackProgress ?? 0 },
            set: { pendingSeekProgress = $0 }
        )
    }

    internal var seekTimeText: String {
        guard let progress = pendingSeekProgress,
              let track = model.playback.nowPlaying,
              let durationMS = track.durationMS else {
            return model.playbackTimeText
        }
        let targetMS = Int(Double(durationMS) * min(max(progress, 0), 1))
        return "\(playbackTimestamp(targetMS)) / \(track.durationText)"
    }
}
#endif
