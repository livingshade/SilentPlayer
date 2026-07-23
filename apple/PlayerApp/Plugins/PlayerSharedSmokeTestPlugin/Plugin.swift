import Foundation
import PackagePlugin

@main
struct PlayerSharedSmokeTestPlugin: BuildToolPlugin {
    func createBuildCommands(context: PluginContext, target: Target) throws -> [Command] {
        let runner = try context.tool(named: "PlayerSharedSmokeTests")
        let outputDirectory = context.pluginWorkDirectoryURL.appending(
            path: "PlayerSharedSmokeTests",
            directoryHint: .isDirectory
        )
        let resultFile = outputDirectory.appending(path: "result.txt")

        return [
            .buildCommand(
                displayName: "Run PlayerShared smoke tests",
                executable: runner.url,
                arguments: [resultFile.path],
                inputFiles: [],
                outputFiles: [resultFile]
            )
        ]
    }
}
