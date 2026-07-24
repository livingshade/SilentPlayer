enum PhoneDisplayText {
    static func compact(_ value: String) -> String {
        value
            .split(whereSeparator: \.isWhitespace)
            .joined(separator: " ")
    }
}
