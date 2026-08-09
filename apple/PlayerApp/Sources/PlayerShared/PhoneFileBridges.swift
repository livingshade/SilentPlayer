#if os(iOS)
import Foundation
import SwiftUI
import UIKit
import UniformTypeIdentifiers

private extension UTType {
    static let silentLibraryPackage = UTType(
        exportedAs: "com.normalplayer.silent-library",
        conformingTo: .package
    )
}

struct PhoneDocumentPickerBridge: UIViewControllerRepresentable {
    @Binding var isPresented: Bool
    let purpose: PhoneFileImportPurpose?
    let onResult: (Result<[URL], Error>) -> Void

    func makeUIViewController(context: Context) -> UIViewController {
        UIViewController()
    }

    func updateUIViewController(_ viewController: UIViewController, context: Context) {
        context.coordinator.parent = self

        guard isPresented, let purpose else {
            if context.coordinator.presentedPicker != nil {
                context.coordinator.dismissPresentedPicker()
            }
            return
        }

        guard context.coordinator.presentedPicker == nil else {
            return
        }

        DispatchQueue.main.async {
            guard isPresented, context.coordinator.presentedPicker == nil else {
                return
            }
            let picker = UIDocumentPickerViewController(
                forOpeningContentTypes: purpose.allowedContentTypes,
                asCopy: purpose.importsAsCopy
            )
            picker.delegate = context.coordinator
            picker.allowsMultipleSelection = purpose.allowsMultipleSelection
            picker.shouldShowFileExtensions = true
            context.coordinator.presentedPicker = picker
            context.coordinator.topPresenter(from: viewController).present(picker, animated: true)
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    final class Coordinator: NSObject, UIDocumentPickerDelegate {
        var parent: PhoneDocumentPickerBridge
        weak var presentedPicker: UIDocumentPickerViewController?

        init(parent: PhoneDocumentPickerBridge) {
            self.parent = parent
        }

        func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
            presentedPicker = nil
            parent.isPresented = false
            parent.onResult(.success(urls))
        }

        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
            presentedPicker = nil
            parent.isPresented = false
            parent.onResult(.success([]))
        }

        func dismissPresentedPicker() {
            presentedPicker?.dismiss(animated: true)
            presentedPicker = nil
        }

        func topPresenter(from viewController: UIViewController) -> UIViewController {
            var presenter = viewController.view.window?.rootViewController ?? viewController
            while let presented = presenter.presentedViewController {
                presenter = presented
            }
            return presenter
        }
    }
}

struct PhoneDocumentExporterBridge: UIViewControllerRepresentable {
    @Binding var isPresented: Bool
    let sourceURL: URL?
    let onResult: (Result<[URL], Error>) -> Void

    func makeUIViewController(context: Context) -> UIViewController {
        UIViewController()
    }

    func updateUIViewController(_ viewController: UIViewController, context: Context) {
        context.coordinator.parent = self

        guard isPresented, let sourceURL else {
            if context.coordinator.presentedPicker != nil {
                context.coordinator.dismissPresentedPicker()
            }
            return
        }

        guard context.coordinator.presentedPicker == nil else {
            return
        }

        DispatchQueue.main.async {
            guard isPresented, context.coordinator.presentedPicker == nil else {
                return
            }
            let picker = UIDocumentPickerViewController(
                forExporting: [sourceURL],
                asCopy: true
            )
            picker.delegate = context.coordinator
            picker.shouldShowFileExtensions = true
            context.coordinator.presentedPicker = picker
            context.coordinator.topPresenter(from: viewController).present(picker, animated: true)
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    final class Coordinator: NSObject, UIDocumentPickerDelegate {
        var parent: PhoneDocumentExporterBridge
        weak var presentedPicker: UIDocumentPickerViewController?

        init(parent: PhoneDocumentExporterBridge) {
            self.parent = parent
        }

        func documentPicker(
            _ controller: UIDocumentPickerViewController,
            didPickDocumentsAt urls: [URL]
        ) {
            presentedPicker = nil
            parent.isPresented = false
            parent.onResult(.success(urls))
        }

        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
            presentedPicker = nil
            parent.isPresented = false
            parent.onResult(.success([]))
        }

        func dismissPresentedPicker() {
            presentedPicker?.dismiss(animated: true)
            presentedPicker = nil
        }

        func topPresenter(from viewController: UIViewController) -> UIViewController {
            var presenter = viewController.view.window?.rootViewController ?? viewController
            while let presented = presenter.presentedViewController {
                presenter = presented
            }
            return presenter
        }
    }
}

enum PhoneFileImportPurpose: Equatable {
    case musicFiles
    case musicFolder
    case libraryPackage
    case trackCover(TrackItem)
    case albumCover(TrackItem)
    case playlistCover(PlaylistItem)
    case playlistSettingsArtwork
    case editArtwork
    case editLyrics

    static let emptyLibraryPrimaryAction = PhoneFileImportPurpose.libraryPackage

    var allowedContentTypes: [UTType] {
        switch self {
        case .musicFiles:
            // OGG/FLAC can arrive as dynamic UTTypes on iOS, so Rust owns the final audio filter.
            return [.item]
        case .musicFolder:
            return [.folder]
        case .libraryPackage:
            return [.silentLibraryPackage, .package]
        case .trackCover, .albumCover, .playlistCover, .playlistSettingsArtwork, .editArtwork:
            return [.image]
        case .editLyrics:
            return [
                UTType(filenameExtension: "lrc") ?? .plainText,
                UTType(filenameExtension: "lyrics") ?? .plainText,
                .plainText
            ]
        }
    }

    var importsAsCopy: Bool {
        switch self {
        case .musicFolder:
            return false
        case .musicFiles, .libraryPackage, .trackCover, .albumCover, .playlistCover, .playlistSettingsArtwork, .editArtwork, .editLyrics:
            return true
        }
    }

    var allowsMultipleSelection: Bool {
        switch self {
        case .musicFiles:
            return true
        case .musicFolder, .libraryPackage, .trackCover, .albumCover, .playlistCover, .playlistSettingsArtwork, .editArtwork, .editLyrics:
            return false
        }
    }

    var presentationStatus: String {
        switch self {
        case .musicFiles:
            return "Choose music files"
        case .musicFolder:
            return "Choose a music folder"
        case .libraryPackage:
            return "Choose a Silent library package"
        case .trackCover:
            return "Choose track artwork"
        case .albumCover:
            return "Choose album artwork"
        case .playlistCover, .playlistSettingsArtwork:
            return "Choose playlist artwork"
        case .editArtwork:
            return "Choose song artwork"
        case .editLyrics:
            return "Choose lyrics file"
        }
    }
}

#endif
