import Combine
import Foundation

@MainActor
public protocol PlaybackSystemIntegration: AnyObject {
    func start()
    func prepareForPlayback() throws
    func applicationDidEnterBackground() throws
    func playbackPositionDidChange()
    func playbackDidStop()
    func shutdown()
}

public extension PlaybackSystemIntegration {
    func applicationDidEnterBackground() throws {}
    func playbackPositionDidChange() {}
}

struct PlaybackNowPlayingObservedState: Equatable {
    let nowPlaying: TrackItem?
    let nowPlayingDetails: TrackDetails?
    let isPlaying: Bool
}

enum PlaybackNowPlayingObservation {
    @MainActor
    static func publisher(
        for model: AppModel
    ) -> AnyPublisher<PlaybackNowPlayingObservedState, Never> {
        Publishers.CombineLatest3(
            model.$nowPlaying.removeDuplicates(),
            model.$nowPlayingDetails.removeDuplicates(),
            model.$isPlaying.removeDuplicates()
        )
        .map { nowPlaying, nowPlayingDetails, isPlaying in
            PlaybackNowPlayingObservedState(
                nowPlaying: nowPlaying,
                nowPlayingDetails: nowPlayingDetails,
                isPlaying: isPlaying
            )
        }
        .removeDuplicates()
        .eraseToAnyPublisher()
    }
}

public enum PlaybackInterruptionPolicy {
    public static func shouldPrepareForResume(
        systemShouldResume: Bool,
        resumeWasScheduled: Bool
    ) -> Bool {
        systemShouldResume && resumeWasScheduled
    }
}

public enum PlaybackRouteChangePolicy {
    public static func shouldPause(
        oldDeviceBecameUnavailable: Bool,
        previousRouteHadPrivateOutput: Bool
    ) -> Bool {
        oldDeviceBecameUnavailable && previousRouteHadPrivateOutput
    }
}

public enum PlaybackRemoteCommandPolicy {
    public static func canPlay(
        hasTrack: Bool,
        isInterrupted: Bool
    ) -> Bool {
        hasTrack && !isInterrupted
    }

    public static func canPause(
        hasTrack: Bool,
        isInterrupted: Bool
    ) -> Bool {
        hasTrack && !isInterrupted
    }

    public static func canTogglePlayPause(
        hasTrack: Bool,
        isInterrupted: Bool
    ) -> Bool {
        hasTrack && !isInterrupted
    }
}

public enum PlaybackNowPlayingPolicy {
    public static func playbackRate(isPlaying: Bool) -> Double {
        isPlaying ? 1 : 0
    }
}

#if os(iOS)
import AVFoundation
import MediaPlayer
import UIKit

private final class IOSArtworkImageBox: @unchecked Sendable {
    let image: UIImage

    init(image: UIImage) {
        self.image = image
    }
}

enum IOSNowPlayingArtworkFactory {
    nonisolated static func make(image: UIImage) -> MPMediaItemArtwork {
        let imageBox = IOSArtworkImageBox(image: image)
        return MPMediaItemArtwork(boundsSize: image.size) { @Sendable _ in
            imageBox.image
        }
    }
}

@MainActor
public final class IOSPlaybackSystemIntegration: NSObject, PlaybackSystemIntegration {
    private weak var model: AppModel?
    private let audioSession = AVAudioSession.sharedInstance()
    private var cancellables = Set<AnyCancellable>()
    private var notificationObservers: [NSObjectProtocol] = []
    private var remoteTargets: [(command: MPRemoteCommand, target: Any)] = []
    private var artworkCache: (path: String, artwork: MPMediaItemArtwork)?
    private var pendingNowPlayingUpdate: Task<Void, Never>?
    private var isStarted = false

    public init(model: AppModel) {
        self.model = model
        super.init()
    }

    public func start() {
        guard !isStarted else {
            return
        }
        isStarted = true

        try? configureAudioSession()
        observePlaybackState()
        observeAudioSession()
        registerRemoteCommands()
        updateNowPlayingInfo()
    }

    public func prepareForPlayback() throws {
        try configureAudioSession()
        try audioSession.setActive(true)
    }

    public func applicationDidEnterBackground() throws {
        guard model?.isPlaying == true else {
            return
        }
        try prepareForPlayback()
    }

    public func playbackPositionDidChange() {
        updateNowPlayingInfo()
    }

    public func playbackDidStop() {
        MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
        try? audioSession.setActive(false, options: .notifyOthersOnDeactivation)
    }

    public func shutdown() {
        guard isStarted else {
            return
        }
        isStarted = false

        pendingNowPlayingUpdate?.cancel()
        pendingNowPlayingUpdate = nil
        cancellables.removeAll()
        for observer in notificationObservers {
            NotificationCenter.default.removeObserver(observer)
        }
        notificationObservers.removeAll()
        for remoteTarget in remoteTargets {
            remoteTarget.command.removeTarget(remoteTarget.target)
        }
        remoteTargets.removeAll()
        playbackDidStop()
    }

    private func configureAudioSession() throws {
        try audioSession.setCategory(
            .playback,
            mode: .default,
            policy: .longFormAudio,
            options: []
        )
    }

    private func observePlaybackState() {
        guard let model else {
            return
        }

        PlaybackNowPlayingObservation.publisher(for: model)
            .dropFirst()
            .sink { [weak self] _ in
                Task { @MainActor [weak self] in
                    self?.scheduleNowPlayingUpdate()
                }
            }
            .store(in: &cancellables)

        Publishers.CombineLatest4(
            model.$queueCount.removeDuplicates(),
            model.$isAudioInterrupted.removeDuplicates(),
            model.$repeatMode.removeDuplicates(),
            model.$isShuffleEnabled.removeDuplicates()
        )
        .dropFirst()
        .sink { [weak self] _, _, _, _ in
            Task { @MainActor [weak self] in
                self?.updateRemoteCommandAvailability()
            }
        }
        .store(in: &cancellables)
    }

    private func scheduleNowPlayingUpdate() {
        pendingNowPlayingUpdate?.cancel()
        pendingNowPlayingUpdate = Task { @MainActor [weak self] in
            await Task.yield()
            guard !Task.isCancelled else {
                return
            }
            self?.updateNowPlayingInfo()
            self?.pendingNowPlayingUpdate = nil
        }
    }

    private func observeAudioSession() {
        let center = NotificationCenter.default
        notificationObservers.append(
            center.addObserver(
                forName: AVAudioSession.interruptionNotification,
                object: audioSession,
                queue: .main
            ) { [weak model] notification in
                guard
                    let typeValue = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                    let type = AVAudioSession.InterruptionType(rawValue: typeValue)
                else {
                    return
                }

                switch type {
                case .began:
                    Task { @MainActor [weak model] in
                        await model?.handleAudioInterruptionBegan()
                    }
                case .ended:
                    let optionsValue = notification.userInfo?[AVAudioSessionInterruptionOptionKey] as? UInt
                    let shouldResume = optionsValue.map {
                        AVAudioSession.InterruptionOptions(rawValue: $0).contains(.shouldResume)
                    } ?? false
                    Task { @MainActor [weak model] in
                        await model?.handleAudioInterruptionEnded(systemShouldResume: shouldResume)
                    }
                @unknown default:
                    break
                }
            }
        )
        notificationObservers.append(
            center.addObserver(
                forName: AVAudioSession.routeChangeNotification,
                object: audioSession,
                queue: .main
            ) { [weak model] notification in
                guard
                    let reasonValue = notification.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt,
                    let reason = AVAudioSession.RouteChangeReason(rawValue: reasonValue)
                else {
                    return
                }

                let previousRoute = notification.userInfo?[AVAudioSessionRouteChangePreviousRouteKey]
                    as? AVAudioSessionRouteDescription
                let previousRouteHadPrivateOutput = previousRoute?.outputs.contains {
                    switch $0.portType {
                    case .headphones, .bluetoothA2DP, .bluetoothHFP, .bluetoothLE:
                        return true
                    default:
                        return false
                    }
                } ?? false
                guard PlaybackRouteChangePolicy.shouldPause(
                    oldDeviceBecameUnavailable: reason == .oldDeviceUnavailable,
                    previousRouteHadPrivateOutput: previousRouteHadPrivateOutput
                ) else {
                    return
                }

                Task { @MainActor [weak model] in
                    await model?.handleAudioOutputDisconnected()
                }
            }
        )
    }

    private func registerRemoteCommands() {
        let center = MPRemoteCommandCenter.shared()

        register(center.playCommand) { model, _ in
            guard !model.isPlaying else {
                return
            }
            await model.pauseOrResume()
        }
        register(center.pauseCommand) { model, _ in
            guard model.isPlaying else {
                return
            }
            await model.pauseOrResume()
        }
        register(center.togglePlayPauseCommand) { model, _ in
            await model.pauseOrResume()
        }
        register(center.nextTrackCommand) { model, _ in
            await model.nextTrack()
        }
        register(center.previousTrackCommand) { model, _ in
            await model.previousTrack()
        }
        register(
            center.changePlaybackPositionCommand,
            acceptsEvent: { $0 is MPChangePlaybackPositionCommandEvent }
        ) { model, event in
            guard let event = event as? MPChangePlaybackPositionCommandEvent else {
                return
            }
            await model.seek(toMilliseconds: Int(event.positionTime * 1_000))
        }
        register(
            center.changeRepeatModeCommand,
            acceptsEvent: { $0 is MPChangeRepeatModeCommandEvent }
        ) { model, event in
            guard let event = event as? MPChangeRepeatModeCommandEvent else {
                return
            }
            let mode: PlaybackRepeatMode
            switch event.repeatType {
            case .all:
                mode = .all
            case .one:
                mode = .one
            default:
                mode = .off
            }
            await model.setRepeatMode(mode)
        }
        register(
            center.changeShuffleModeCommand,
            acceptsEvent: { $0 is MPChangeShuffleModeCommandEvent }
        ) { model, event in
            guard let event = event as? MPChangeShuffleModeCommandEvent else {
                return
            }
            let shouldEnable = event.shuffleType == .items
            if model.isShuffleEnabled != shouldEnable {
                await model.toggleShuffle()
            }
        }
    }

    private func register(
        _ command: MPRemoteCommand,
        acceptsEvent: @escaping (MPRemoteCommandEvent) -> Bool = { _ in true },
        operation: @escaping @MainActor (AppModel, MPRemoteCommandEvent) async -> Void
    ) {
        let target = command.addTarget { [weak model] event in
            guard acceptsEvent(event) else {
                return .commandFailed
            }
            guard model != nil else {
                return .noActionableNowPlayingItem
            }
            Task { @MainActor [weak model] in
                guard let model else {
                    return
                }
                await operation(model, event)
            }
            return .success
        }
        remoteTargets.append((command, target))
    }

    private func updateRemoteCommandAvailability() {
        guard let model else {
            return
        }

        let center = MPRemoteCommandCenter.shared()
        let hasTrack = model.nowPlaying != nil
        center.playCommand.isEnabled = PlaybackRemoteCommandPolicy.canPlay(
            hasTrack: hasTrack,
            isInterrupted: model.isAudioInterrupted
        )
        center.pauseCommand.isEnabled = PlaybackRemoteCommandPolicy.canPause(
            hasTrack: hasTrack,
            isInterrupted: model.isAudioInterrupted
        )
        center.togglePlayPauseCommand.isEnabled =
            PlaybackRemoteCommandPolicy.canTogglePlayPause(
                hasTrack: hasTrack,
                isInterrupted: model.isAudioInterrupted
            )
        center.nextTrackCommand.isEnabled = model.queueCount > 1
        center.previousTrackCommand.isEnabled = model.queueCount > 1
        center.changePlaybackPositionCommand.isEnabled = model.nowPlaying?.durationMS != nil
        center.changeRepeatModeCommand.isEnabled = hasTrack
        center.changeRepeatModeCommand.currentRepeatType = switch model.repeatMode {
        case .off: .off
        case .all: .all
        case .one: .one
        }
        center.changeShuffleModeCommand.isEnabled = model.queueCount > 1
        center.changeShuffleModeCommand.currentShuffleType = model.isShuffleEnabled ? .items : .off
    }

    private func updateNowPlayingInfo() {
        guard let model, let track = model.nowPlaying else {
            playbackDidStop()
            updateRemoteCommandAvailability()
            return
        }

        var info: [String: Any] = [
            MPMediaItemPropertyTitle: track.title,
            MPMediaItemPropertyArtist: track.artist,
            MPMediaItemPropertyAlbumTitle: track.album,
            MPNowPlayingInfoPropertyElapsedPlaybackTime: Double(model.playbackElapsedMS) / 1_000,
            MPNowPlayingInfoPropertyPlaybackRate: PlaybackNowPlayingPolicy.playbackRate(
                isPlaying: model.isPlaying
            ),
            MPNowPlayingInfoPropertyDefaultPlaybackRate: 1.0,
            MPNowPlayingInfoPropertyMediaType: MPNowPlayingInfoMediaType.audio.rawValue
        ]
        if let durationMS = track.durationMS {
            info[MPMediaItemPropertyPlaybackDuration] = Double(durationMS) / 1_000
        }

        let detailsArtworkURL = model.nowPlayingDetails.flatMap { details in
            details.identity == track.identity ? details.artworkURL : nil
        }
        if let artwork = artwork(for: detailsArtworkURL ?? track.artworkURL) {
            info[MPMediaItemPropertyArtwork] = artwork
        }

        MPNowPlayingInfoCenter.default().nowPlayingInfo = info
        updateRemoteCommandAvailability()
    }

    private func artwork(for url: URL?) -> MPMediaItemArtwork? {
        guard let url else {
            artworkCache = nil
            return nil
        }
        if artworkCache?.path == url.path {
            return artworkCache?.artwork
        }
        guard let image = UIImage(contentsOfFile: url.path) else {
            artworkCache = nil
            return nil
        }

        let artwork = IOSNowPlayingArtworkFactory.make(image: image)
        artworkCache = (url.path, artwork)
        return artwork
    }
}
#endif
