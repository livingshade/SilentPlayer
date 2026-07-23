// swift-tools-version: 6.0

import Foundation
import PackageDescription

let packageRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let repoRoot = packageRoot
    .deletingLastPathComponent()
    .deletingLastPathComponent()
let rustReleasePath = repoRoot.appendingPathComponent("target/release").path
let rustDebugPath = repoRoot.appendingPathComponent("target/debug").path
let playerFFIFrameworkPath = packageRoot
    .appendingPathComponent("Vendor/PlayerFFI.xcframework")
    .path
let hasPlayerFFIFramework = FileManager.default.fileExists(atPath: playerFFIFrameworkPath)
let playerRustFFIDependencies: [Target.Dependency] = hasPlayerFFIFramework
    ? [.target(name: "PlayerFFIBinary", condition: .when(platforms: [.iOS]))]
    : []
let playerFFIBinaryTargets: [Target] = hasPlayerFFIFramework
    ? [
        .binaryTarget(
            name: "PlayerFFIBinary",
            path: "Vendor/PlayerFFI.xcframework"
        )
    ]
    : []
let macAppRustLinkerSettings: [LinkerSetting] = [
    .unsafeFlags([
        "-L", rustDebugPath,
        "-lplayer_ffi",
        "-Xlinker", "-rpath",
        "-Xlinker", "@executable_path"
    ], .when(platforms: [.macOS], configuration: .debug)),
    .unsafeFlags([
        "-L", rustReleasePath,
        "-lplayer_ffi",
        "-Xlinker", "-rpath",
        "-Xlinker", "@executable_path"
    ], .when(platforms: [.macOS], configuration: .release))
]
let macTestRustLinkerSettings: [LinkerSetting] = [
    .unsafeFlags([
        "-L", rustDebugPath,
        "-lplayer_ffi",
        "-Xlinker", "-rpath",
        "-Xlinker", rustDebugPath
    ], .when(platforms: [.macOS], configuration: .debug)),
    .unsafeFlags([
        "-L", rustReleasePath,
        "-lplayer_ffi",
        "-Xlinker", "-rpath",
        "-Xlinker", rustReleasePath
    ], .when(platforms: [.macOS], configuration: .release))
]

let package = Package(
    name: "NormalPlayerApple",
    platforms: [
        .macOS(.v13),
        .iOS(.v16)
    ],
    products: [
        .executable(name: "Silent", targets: ["Silent"]),
        .executable(name: "NormalPlayer-iOS", targets: ["NormalPlayeriOS"]),
        .executable(name: "PlayerSharedSmokeTests", targets: ["PlayerSharedSmokeTests"])
    ],
    targets: [
        .target(
            name: "PlayerRustFFI",
            dependencies: playerRustFFIDependencies,
            publicHeadersPath: "include"
        ),
        .target(
            name: "PlayerShared",
            dependencies: ["PlayerRustFFI"]
        ),
        .executableTarget(
            name: "Silent",
            dependencies: ["PlayerShared"],
            path: "Sources/PlayerMac",
            linkerSettings: macAppRustLinkerSettings
        ),
        .executableTarget(
            name: "NormalPlayeriOS",
            dependencies: ["PlayerShared"],
            path: "Sources/PlayeriOS",
            linkerSettings: [
                .linkedFramework("AudioToolbox", .when(platforms: [.iOS]))
            ] + macAppRustLinkerSettings
        ),
        .executableTarget(
            name: "PlayerSharedSmokeTests",
            dependencies: ["PlayerShared"],
            linkerSettings: macTestRustLinkerSettings
        ),
        .plugin(
            name: "PlayerSharedSmokeTestPlugin",
            capability: .buildTool(),
            dependencies: ["PlayerSharedSmokeTests"]
        ),
        .testTarget(
            name: "PlayerSharedTests",
            dependencies: ["PlayerShared"],
            linkerSettings: macTestRustLinkerSettings,
            plugins: [
                .plugin(name: "PlayerSharedSmokeTestPlugin")
            ]
        )
    ] + playerFFIBinaryTargets
)
