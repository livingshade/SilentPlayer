import Foundation
import XCTest
@testable import PlayerShared

#if os(iOS)
import UniformTypeIdentifiers
#endif

final class PhoneDisplayTextTests: XCTestCase {
    func testCollapsesImportedLineBreaksAndWhitespace() {
        XCTAssertEqual(
            PhoneDisplayText.compact("  A title\nwith\tmetadata   spacing  "),
            "A title with metadata spacing"
        )
    }
}

final class PhonePresentationStateTests: XCTestCase {
    #if os(iOS)
    func testEmptyPhoneLibraryActionConfiguresASingleCopiedLibraryPackageImport() {
        let purpose = PhoneFileImportPurpose.emptyLibraryPrimaryAction

        XCTAssertEqual(
            purpose.allowedContentTypes.map(\.identifier),
            ["com.normalplayer.silent-library", UTType.package.identifier]
        )
        XCTAssertTrue(purpose.importsAsCopy)
        XCTAssertFalse(purpose.allowsMultipleSelection)
        XCTAssertEqual(purpose.presentationStatus, "Choose a Silent library package")
    }
    #endif

    func testSnapshotRoundTripsThroughSceneStorageEncoding() throws {
        let snapshot = PhonePresentationSnapshot(
            selectedTab: .playlists,
            contentScope: .playlist(42),
            playlistDetailID: 42,
            selectedTrackID: "track:favorite"
        )

        let encoded = try XCTUnwrap(PhonePresentationPersistence.encode(snapshot))

        XCTAssertEqual(PhonePresentationPersistence.decode(encoded), snapshot)
        XCTAssertEqual(snapshot.bootstrapScope, .playlist(42))
    }

    func testDeletedPlaylistFallsBackToLibraryAndClearsDetailRoute() {
        let snapshot = PhonePresentationSnapshot(
            selectedTab: .playlists,
            contentScope: .playlist(42),
            playlistDetailID: 42,
            selectedTrackID: nil
        )

        let validated = snapshot.validated(against: [])

        XCTAssertEqual(validated.contentScope, .library)
        XCTAssertNil(validated.playlistDetailID)
    }
}
final class MacPresentationStateTests: XCTestCase {
    func testSnapshotRoundTripsThroughSceneStorageEncoding() throws {
        let snapshot = MacPresentationSnapshot(
            contentScope: .playlist(73),
            selectedTrackID: "track:studio"
        )

        let encoded = try XCTUnwrap(MacPresentationPersistence.encode(snapshot))

        XCTAssertEqual(MacPresentationPersistence.decode(encoded), snapshot)
    }

    func testDeletedPlaylistFallsBackToLibrary() {
        let snapshot = MacPresentationSnapshot(
            contentScope: .playlist(73),
            selectedTrackID: "track:studio"
        )

        let validated = snapshot.validated(against: [])

        XCTAssertEqual(validated.contentScope, .library)
        XCTAssertEqual(validated.selectedTrackID, "track:studio")
    }

}
