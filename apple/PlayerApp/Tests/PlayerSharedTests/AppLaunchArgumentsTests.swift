import XCTest
@testable import PlayerShared

final class AppLaunchArgumentsTests: XCTestCase {
    func test_cliInvocationIsRecognizedForNonCliAppEntryPoints() {
        XCTAssertTrue(AppLaunchArguments.isCLIInvocation(["Silent", "--cli", "library", "list"]))
    }

    func test_normalAppArgumentsAreNotRecognizedAsCli() {
        XCTAssertFalse(AppLaunchArguments.isCLIInvocation(["Silent", "--help"]))
        XCTAssertFalse(AppLaunchArguments.isCLIInvocation(["Silent"]))
    }
}
