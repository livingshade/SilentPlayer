import Combine
import Foundation

@MainActor
extension AppModel {
    public func configureDiscordPresence(enabled: Bool, applicationID: String) async {
        guard enabled else {
            await disableDiscordPresence()
            return
        }
        let applicationID = applicationID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !applicationID.isEmpty else {
            discordPresenceStatus = "Enter a Discord Application ID"
            isDiscordPresenceSharing = false
            return
        }
        do {
            _ = try await invoke { try $0.configureDiscordPresence(applicationID: applicationID) }
            await testDiscordPresence()
            await syncDiscordPresence()
        } catch {
            setDiscordPresenceFailure(error)
        }
    }

    public func testDiscordPresence() async {
        do {
            let status = try await invoke { try $0.testDiscordPresence() }
            applyDiscordPresenceStatus(status, connectedText: "Connected to Discord")
        } catch {
            setDiscordPresenceFailure(error)
        }
    }

    internal func syncDiscordPresence() async {
        do {
            let status = try await invoke(priority: .utility) { try $0.syncDiscordPresence() }
            let connectedText = status.sharingTrack ? "Sharing current track" : "Ready"
            applyDiscordPresenceStatus(status, connectedText: connectedText)
        } catch {
            setDiscordPresenceFailure(error)
        }
    }

    internal func shutdownDiscordPresence() {
        guard let client else {
            return
        }
        _ = try? client.disableDiscordPresence()
        discordPresenceStatus = "Off"
        isDiscordPresenceSharing = false
    }

    private func disableDiscordPresence() async {
        do {
            let status = try await invoke { try $0.disableDiscordPresence() }
            applyDiscordPresenceStatus(status, connectedText: "Off")
        } catch {
            setDiscordPresenceFailure(error)
        }
    }

    private func applyDiscordPresenceStatus(
        _ status: DiscordPresenceStatus,
        connectedText: String
    ) {
        isDiscordPresenceSharing = status.sharingTrack
        if !status.enabled {
            discordPresenceStatus = "Off"
        } else if !status.discordRunning {
            discordPresenceStatus = "Discord desktop is not running"
        } else {
            discordPresenceStatus = connectedText
        }
    }

    private func setDiscordPresenceFailure(_ error: Error) {
        isDiscordPresenceSharing = false
        discordPresenceStatus = error.localizedDescription
    }
}

#if os(macOS)
@MainActor
public final class MacDiscordPresenceIntegration: PlaybackSystemIntegration {
    private weak var model: AppModel?
    private var cancellables = Set<AnyCancellable>()
    private var pendingSync: Task<Void, Never>?

    public init(model: AppModel) {
        self.model = model
    }

    public func start() {
        guard cancellables.isEmpty, let model else {
            return
        }
        PlaybackNowPlayingObservation.publisher(for: model)
            .dropFirst()
            .sink { [weak self] _ in
                Task { @MainActor [weak self] in
                    self?.scheduleSync()
                }
            }
            .store(in: &cancellables)
    }

    public func prepareForPlayback() throws {}

    public func playbackPositionDidChange() {
        scheduleSync()
    }

    public func playbackDidStop() {
        scheduleSync()
    }

    public func shutdown() {
        pendingSync?.cancel()
        pendingSync = nil
        cancellables.removeAll()
        model?.shutdownDiscordPresence()
    }

    private func scheduleSync() {
        pendingSync?.cancel()
        pendingSync = Task { @MainActor [weak self] in
            await Task.yield()
            guard !Task.isCancelled, let self, let model = self.model else {
                return
            }
            await model.syncDiscordPresence()
            pendingSync = nil
        }
    }
}
#endif
