import Foundation

public enum AppLaunchArguments {
    public static let unsupportedCLIMessage =
        "CLI is not supported by this app. Use the `silent` CLI executable."

    public static func isCLIInvocation(_ arguments: [String]) -> Bool {
        arguments.dropFirst().contains("--cli")
    }
}
