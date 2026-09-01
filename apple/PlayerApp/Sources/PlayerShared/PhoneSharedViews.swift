#if os(iOS)
import Foundation
import SwiftUI
import UIKit

struct PhoneArtworkImage: View {
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

struct PhoneEmptyState: View {
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

struct PhonePlaybackModeMenu: View {
    @ObservedObject var model: AppModel
    let showsTitle: Bool

    var body: some View {
        Menu {
            ForEach(PlaybackMode.allCases) { mode in
                Button {
                    Task { await model.setPlaybackMode(mode) }
                } label: {
                    Label(
                        mode.label,
                        systemImage: model.playback.playbackMode == mode
                            ? "checkmark"
                            : mode.systemImage
                    )
                }
            }
        } label: {
            menuLabel
        }
        .accessibilityLabel("Playback Order")
        .accessibilityValue(model.playback.playbackMode.label)
    }

    @ViewBuilder
    private var menuLabel: some View {
        if showsTitle {
            Label(
                model.playback.playbackMode.label,
                systemImage: model.playback.playbackMode.systemImage
            )
        } else {
            Image(systemName: model.playback.playbackMode.systemImage)
                .foregroundStyle(
                    model.playback.playbackMode == .sequential
                        ? Color.secondary
                        : Color.accentColor
                )
        }
    }
}
#endif
