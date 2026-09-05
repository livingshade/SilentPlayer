#if os(macOS)
import AppKit
import CoreText
import SwiftUI

enum RubyLyricsTextLayout {
    static func utf16Range(
        in text: String,
        startCharacter: Int,
        endCharacter: Int
    ) -> NSRange? {
        guard startCharacter >= 0,
              startCharacter < endCharacter,
              let start = text.index(
                  text.startIndex,
                  offsetBy: startCharacter,
                  limitedBy: text.endIndex
              ),
              let end = text.index(
                  text.startIndex,
                  offsetBy: endCharacter,
                  limitedBy: text.endIndex
              ) else {
            return nil
        }
        return NSRange(start..<end, in: text)
    }

    static func attributedString(
        for line: LyricsPresentationLine,
        font: NSFont,
        color: NSColor
    ) -> NSAttributedString {
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.alignment = .center
        paragraphStyle.lineBreakMode = .byTruncatingTail
        let result = NSMutableAttributedString(
            string: line.text,
            attributes: [
                .font: font,
                .foregroundColor: color,
                .paragraphStyle: paragraphStyle
            ]
        )
        let rubyKey = NSAttributedString.Key(
            rawValue: kCTRubyAnnotationAttributeName as String
        )
        for item in line.ruby {
            guard let range = utf16Range(
                in: line.text,
                startCharacter: item.startCharacter,
                endCharacter: item.endCharacter
            ) else {
                continue
            }
            let annotation = CTRubyAnnotationCreateWithAttributes(
                .auto,
                .auto,
                .before,
                item.reading as CFString,
                [
                    kCTRubyAnnotationSizeFactorAttributeName: 0.5,
                    kCTRubyAnnotationScaleToFitAttributeName: true
                ] as CFDictionary
            )
            result.addAttribute(rubyKey, value: annotation, range: range)
        }
        return result
    }

    static func typographicWidth(
        for line: LyricsPresentationLine,
        font: NSFont
    ) -> CGFloat {
        let attributed = attributedString(
            for: line,
            font: font,
            color: .labelColor
        )
        let coreTextLine = CTLineCreateWithAttributedString(attributed)
        return CGFloat(CTLineGetTypographicBounds(coreTextLine, nil, nil, nil))
    }
}

struct RubyLyricsText: NSViewRepresentable {
    let line: LyricsPresentationLine
    let font: NSFont
    let color: NSColor

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField(labelWithString: "")
        field.alignment = .center
        field.maximumNumberOfLines = 1
        field.lineBreakMode = .byTruncatingTail
        field.allowsDefaultTighteningForTruncation = true
        return field
    }

    func updateNSView(_ nsView: NSTextField, context: Context) {
        nsView.attributedStringValue = RubyLyricsTextLayout.attributedString(
            for: line,
            font: font,
            color: color
        )
        nsView.toolTip = line.text
        nsView.setAccessibilityLabel(line.text)
    }
}
#endif
