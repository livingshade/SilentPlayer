#if os(iOS)
import PlayerShared
import SwiftUI
@main
struct NormalPlayeriOSApp: App {
    @StateObject private var model = AppModel()
    @State private var isSystemPlaybackInstalled = false

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
        print("NormalPlayer-iOS is only available when building for iOS.")
    }
}
#endif
