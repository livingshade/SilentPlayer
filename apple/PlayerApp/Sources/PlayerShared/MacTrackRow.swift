#if os(macOS)
import Foundation
import SwiftUI

struct TrackRow: View {
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
#endif
