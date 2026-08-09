import Combine
import Foundation
import XCTest
@testable import PlayerShared

@MainActor
final class FeatureStateAndWorkerProtocolTests: XCTestCase {
    func testRootModelForwardsFeatureStateChanges() {
        let model = AppModel(client: nil, discoverClient: { throw TestError.unavailable })
        var changes = 0
        let observation = model.objectWillChange.sink { changes += 1 }

        model.operations.status = "Updated"

        XCTAssertEqual(model.operations.status, "Updated")
        XCTAssertEqual(changes, 1)
        withExtendedLifetime(observation) {}
    }

    #if os(macOS)
    func testAnalyzerProtocolRejectsMissingRequiredFields() throws {
        let event = try decode(AnalyzerWorkerEventDTO.self, """
        {"event":"track_finished","index":1,"total":2,"title":"Song"}
        """)

        XCTAssertThrowsError(try event.model())
    }

    func testLibraryProtocolRejectsUnknownEvents() throws {
        let event = try decode(LibraryWorkerEventDTO.self, """
        {"event":"new_event","operation":"audit"}
        """)

        XCTAssertThrowsError(try event.model())
    }

    func testLibraryProtocolProducesTypedAuditResult() throws {
        let event = try decode(LibraryWorkerEventDTO.self, """
        {
          "event":"finished",
          "operation":"audit",
          "total":3,
          "imported":0,
          "copied":0,
          "duplicates_skipped":0,
          "artwork_cached":0,
          "metadata_warnings":0,
          "tracks_scanned":3,
          "hashes_updated":2,
          "duplicate_groups":1,
          "tracks_merged":1,
          "failures":0
        }
        """)

        XCTAssertEqual(
            try event.model(),
            .auditFinished(
                tracksScanned: 3,
                hashesUpdated: 2,
                duplicateGroups: 1,
                tracksMerged: 1,
                failures: 0
            )
        )
    }

    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(type, from: Data(json.utf8))
    }
    #endif
}

private enum TestError: Error {
    case unavailable
}
