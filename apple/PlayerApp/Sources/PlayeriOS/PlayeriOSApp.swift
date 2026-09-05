import Darwin
import PlayerShared

#if os(iOS)
import SwiftUI
@main
struct NormalPlayeriOSApp: App {
    @StateObject private var model = AppModel()
    @State private var isSystemPlaybackInstalled = false

    init() {
        if AppLaunchArguments.isCLIInvocation(CommandLine.arguments) {
            print(AppLaunchArguments.unsupportedCLIMessage)
            exit(2)
        }
    }

    var body: some Scene {
        WindowGroup {
            PhoneContentView(model: model)
                .onAppear {
                    guard !isSystemPlaybackInstalled else {
                        return
                    }
                    isSystemPlaybackInstalled = true
                    model.installPlaybackSystemIntegration(
                        IOSPlaybackSystemIntegration(model: model)
                    )
                }
        }
    }
}
#else
@main
struct NormalPlayeriOSBuildStub {
    static func main() {
        if AppLaunchArguments.isCLIInvocation(CommandLine.arguments) {
            print(AppLaunchArguments.unsupportedCLIMessage)
            exit(2)
        }
        print("NormalPlayer-iOS is only available when building for iOS.")
    }
}
#endif
