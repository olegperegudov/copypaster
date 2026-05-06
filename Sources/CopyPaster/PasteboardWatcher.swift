// Polls NSPasteboard.changeCount каждые 500ms.
// Когда счётчик меняется — забираем картинку (png/tiff) или строку и эмитим в store.

import AppKit

final class PasteboardWatcher {
    private let pb = NSPasteboard.general
    private var lastChange: Int
    private var timer: Timer?
    private let onNew: (ClipItem) -> Void

    // Когда мы сами кладём что-то на pasteboard (Paster, ScreenshotWatcher) —
    // следующий tick должен этот change проглотить, иначе попадёт в историю дублем.
    var skipNextChange = false

    init(onNew: @escaping (ClipItem) -> Void) {
        self.lastChange = NSPasteboard.general.changeCount
        self.onNew = onNew
    }

    func start() {
        timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            self?.poll()
        }
    }

    private func poll() {
        guard pb.changeCount != lastChange else { return }
        let cc = pb.changeCount
        lastChange = cc

        if skipNextChange {
            skipNextChange = false
            Log.write("[watcher]", "change=\(cc) skipped (own paste)")
            return
        }

        if let data = pb.data(forType: .png), let img = NSImage(data: data) {
            Log.write("[watcher]", "change=\(cc) → image(png, \(data.count)B)")
            onNew(ClipItem(kind: .image(img)))
            return
        }
        if let data = pb.data(forType: .tiff), let img = NSImage(data: data) {
            Log.write("[watcher]", "change=\(cc) → image(tiff, \(data.count)B)")
            onNew(ClipItem(kind: .image(img)))
            return
        }
        if let str = pb.string(forType: .string), !str.isEmpty {
            Log.write("[watcher]", "change=\(cc) → text(\(str.prefix(20))…)")
            onNew(ClipItem(kind: .text(str)))
        }
    }
}
