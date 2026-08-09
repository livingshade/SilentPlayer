#if os(macOS)
import Foundation
import SwiftUI

struct LibraryInformationSheet: View {
    let status: String
    let databasePath: String
    let musicPath: String
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                Text("Library Information")
                    .font(.title2.weight(.semibold))
                Spacer()
                Button("Done") {
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
            }

            Divider()

            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 12) {
                informationRow("Status", status)
                informationRow("Database", databasePath)
                informationRow("Music Folder", musicPath)
            }
        }
        .padding(22)
        .frame(minWidth: 560, idealWidth: 640, maxWidth: 760)
    }

    private func informationRow(_ label: String, _ value: String) -> some View {
        GridRow {
            Text(label)
                .foregroundStyle(.secondary)
            Text(value)
                .font(label == "Status" ? .body : .callout.monospaced())
                .lineLimit(3)
                .truncationMode(.middle)
                .textSelection(.enabled)
        }
    }
}
#endif
