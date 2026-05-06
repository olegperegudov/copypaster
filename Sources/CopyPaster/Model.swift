// Model: clipboard item + ring buffer.
// Item — текст или картинка. Store держит последние N в памяти, дедупит подряд идущие дубли.

import AppKit
import SwiftUI

struct ClipItem: Identifiable {
    let id = UUID()
    let kind: Kind
    enum Kind {
        case text(String)
        case image(NSImage)
    }
}

final class HistoryStore: ObservableObject {
    @Published private(set) var items: [ClipItem] = []
    private let maxItems = 20

    func add(_ item: ClipItem) {
        if let last = items.first, contentEqual(last, item) { return }
        items.insert(item, at: 0)
        if items.count > maxItems { items.removeLast() }
    }

    private func contentEqual(_ a: ClipItem, _ b: ClipItem) -> Bool {
        switch (a.kind, b.kind) {
        case let (.text(x), .text(y)):
            return x == y
        case let (.image(x), .image(y)):
            return x.tiffRepresentation == y.tiffRepresentation
        default:
            return false
        }
    }
}
