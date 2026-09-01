import Foundation
import PlayerShared
import SwiftUI

enum DiscordPresencePreferences {
    private static let enabledKey = "discordPresence.enabled"
    private static let applicationIDKey = "discordPresence.applicationID"

    static var isEnabled: Bool {
        UserDefaults.standard.bool(forKey: enabledKey)
    }

    static var applicationID: String {
        UserDefaults.standard.string(forKey: applicationIDKey) ?? ""
    }

    static func save(enabled: Bool, applicationID: String) {
        UserDefaults.standard.set(enabled, forKey: enabledKey)
        UserDefaults.standard.set(applicationID, forKey: applicationIDKey)
    }
}

struct DiscordPresenceSettingsView: View {
    @ObservedObject var model: AppModel
    @State private var isEnabled = DiscordPresencePreferences.isEnabled
    @State private var applicationID = DiscordPresencePreferences.applicationID
    @State private var isApplying = false

    var body: some View {
        Form {
            Toggle("Show the current track on Discord", isOn: $isEnabled)

            TextField("Discord Application ID", text: $applicationID)
                .textFieldStyle(.roundedBorder)
                .disabled(!isEnabled)

            Text("Silent shares only the song title, artist, album, and playback timing. Local file paths are never sent.")
                .font(.caption)
                .foregroundStyle(.secondary)

            HStack {
                Button(isEnabled ? "Save and Test" : "Save") {
                    applySettings()
                }
                .disabled(isApplying)

                if isApplying {
                    ProgressView()
                        .controlSize(.small)
                }

                Spacer()

                Text(model.discordPresenceStatus)
                    .foregroundStyle(model.isDiscordPresenceSharing ? .green : .secondary)
            }
        }
        .formStyle(.grouped)
        .padding()
        .frame(width: 520, height: 250)
    }

    private func applySettings() {
        let normalizedID = applicationID.trimmingCharacters(in: .whitespacesAndNewlines)
        applicationID = normalizedID
        DiscordPresencePreferences.save(enabled: isEnabled, applicationID: normalizedID)
        isApplying = true
        Task { @MainActor in
            await model.configureDiscordPresence(
                enabled: isEnabled,
                applicationID: normalizedID
            )
            isApplying = false
        }
    }
}
