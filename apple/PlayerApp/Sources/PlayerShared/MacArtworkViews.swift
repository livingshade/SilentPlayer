#if os(macOS)
import AppKit
import Foundation
import SwiftUI

struct TrackArtworkThumbnail: View {
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

struct PlaylistArtworkThumbnail: View {
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

struct NowPlayingBackdrop: View {
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

struct ArtworkViewport: View {
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
