import Foundation
import XCTest
@testable import PlayerShared


final class LyricsTimelineTests: XCTestCase {
    private let document = LyricsDocument(
        format: .lrc,
        content: .timed([
            TimedLyricsLine(id: 0, startMS: 1_000, text: "First"),
            TimedLyricsLine(id: 1, startMS: 2_500, text: "Second"),
            TimedLyricsLine(id: 2, startMS: 2_500, text: "Second duplicate"),
        ])
    )

    func testActiveLineUsesRustTimelineBoundarySemantics() {
        XCTAssertNil(document.activeLineIndex(at: 999))
        XCTAssertEqual(document.activeLineIndex(at: 1_000), 0)
        XCTAssertEqual(document.activeLineIndex(at: 2_499), 0)
        XCTAssertEqual(document.activeLineIndex(at: 2_500), 2)
        XCTAssertEqual(document.activeLineIndex(at: Int.max), 2)
    }

    func testPlainLyricsHaveNoActiveTimelineLine() {
        let plain = LyricsDocument(format: .plainText, content: .plain("Static lyrics"))
        XCTAssertNil(plain.activeLineIndex(at: 1_000))
        XCTAssertNil(plain.timedLines)
    }

    func testCompactLineUsesOnlyTheCurrentTimedLyric() {
        XCTAssertEqual(document.compactLine(), "First")
        XCTAssertEqual(
            document.compactLine(at: 999),
            LyricsDocument.defaultInstrumentalToken
        )
        XCTAssertEqual(document.compactLine(at: 1_000), "First")
        XCTAssertEqual(document.compactLine(at: 2_500), "Second duplicate")
    }

    func testCompactLineUsesInstrumentalTokenForEmptyCoverage() {
        let timed = LyricsDocument(
            format: .lrc,
            content: .timed([
                TimedLyricsLine(id: 0, startMS: 0, text: ""),
                TimedLyricsLine(id: 1, startMS: 1_000, text: "  Visible  "),
            ])
        )
        let plain = LyricsDocument(
            format: .plainText,
            content: .plain("\n  First line  \nSecond line")
        )

        XCTAssertTrue(timed.hasDisplayableLyrics)
        XCTAssertEqual(
            timed.compactLine(at: 0),
            LyricsDocument.defaultInstrumentalToken
        )
        XCTAssertEqual(timed.compactLine(at: 1_000), "Visible")
        XCTAssertEqual(plain.compactLine(), "First line")
        XCTAssertEqual(plain.compactLine(at: 1_000), "♪")
        let emptyPlain = LyricsDocument(format: .plainText, content: .plain(" \n "))
        XCTAssertFalse(emptyPlain.hasDisplayableLyrics)
        XCTAssertEqual(
            emptyPlain.compactLine(),
            LyricsDocument.defaultInstrumentalToken
        )

        let instrumental = LyricsDocument.instrumental()
        XCTAssertFalse(instrumental.hasDisplayableLyrics)
        XCTAssertEqual(
            instrumental.compactLine(at: Int.max),
            LyricsDocument.defaultInstrumentalToken
        )
    }

    func testPresentationClockInterpolatesOnlyWhilePlayingAndClampsToDuration() {
        XCTAssertEqual(
            PlaybackPresentationClock.positionMS(
                basePositionMS: 1_000,
                baseUptime: 10,
                currentUptime: 10.25,
                isPlaying: true,
                durationMS: 5_000
            ),
            1_250
        )
        XCTAssertEqual(
            PlaybackPresentationClock.positionMS(
                basePositionMS: 1_000,
                baseUptime: 10,
                currentUptime: 20,
                isPlaying: true,
                durationMS: 5_000
            ),
            5_000
        )
        XCTAssertEqual(
            PlaybackPresentationClock.positionMS(
                basePositionMS: 1_000,
                baseUptime: 10,
                currentUptime: 20,
                isPlaying: false,
                durationMS: 5_000
            ),
            1_000
        )
    }

    func testTrackDetailsDecodeTheStructuredRustLyricsDocument() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let source = root.appendingPathComponent("Source", isDirectory: true)
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let audio = source.appendingPathComponent("song.ogg")
        try FileManager.default.copyItem(
            at: repositoryRoot
                .appendingPathComponent("test-assets/audio/into_the_oceans_chorus.ogg"),
            to: audio
        )
        try "[offset:+100]\n[00:01.00]First\n[00:02.50]Second\n"
            .write(
                to: source.appendingPathComponent("song.lrc"),
                atomically: true,
                encoding: .utf8
            )

        let client = try RustPlayerClient(
            dbURL: root.appendingPathComponent("library.sqlite3"),
            mediaRootURL: root.appendingPathComponent("Music", isDirectory: true),
            repoRoot: repositoryRoot
        )
        _ = try client.importFiles([audio])
        let track = try XCTUnwrap(client.library().first)
        let details = try client.trackDetails(path: track.path)
        let lyrics = try XCTUnwrap(details.lyricsDocument)
        let lines = try XCTUnwrap(lyrics.timedLines)

        XCTAssertEqual(lyrics.format, .lrc)
        XCTAssertEqual(lyrics.instrumentalToken, "♪")
        XCTAssertEqual(lines.map(\.startMS), [1_100, 2_600])
        XCTAssertEqual(lines.map(\.text), ["First", "Second"])
    }
}
