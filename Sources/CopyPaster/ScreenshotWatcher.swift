// Watcher на папку скриншотов macOS.
// Cmd+Shift+3/4 по дефолту сохраняет файл (а не кладёт в буфер). Эта штука детектит
// новый файл и копирует картинку на NSPasteboard — дальше PasteboardWatcher подхватит.
// Папку и basename берём из системных префов com.apple.screencapture.

import AppKit

final class ScreenshotWatcher {
    private let watcher: PasteboardWatcher
    private var seen: Set<String> = []
    private var timer: Timer?
    private let dir: URL
    private let basenames: [String]

    init(watcher: PasteboardWatcher) {
        self.watcher = watcher

        let prefs = UserDefaults(suiteName: "com.apple.screencapture")
        let rawLoc = prefs?.string(forKey: "location")
        let path = (rawLoc as NSString?)?.expandingTildeInPath ?? "\(NSHomeDirectory())/Desktop"
        self.dir = URL(fileURLWithPath: path)

        // Custom basename if set, плюс стандартные локализованные варианты.
        let custom = prefs?.string(forKey: "name")
        self.basenames = [
            custom,
            "Screenshot",
            "Screen Shot",
            "Снимок экрана",
        ].compactMap { $0 }

        self.seen = currentScreenshotFiles()
    }

    func start() {
        timer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.scan()
        }
    }

    private func currentScreenshotFiles() -> Set<String> {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: dir.path) else {
            return []
        }
        return Set(names.filter(isScreenshot))
    }

    private func isScreenshot(_ name: String) -> Bool {
        guard name.lowercased().hasSuffix(".png") else { return false }
        return basenames.contains(where: { name.hasPrefix($0) })
    }

    private func scan() {
        let current = currentScreenshotFiles()
        let new = current.subtracting(seen)
        seen = current

        for name in new {
            Log.write("[screen]", "new file: \(name)")
            let url = dir.appendingPathComponent(name)
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in
                self?.copyToPasteboard(url)
            }
        }
    }

    private func copyToPasteboard(_ url: URL) {
        guard let img = NSImage(contentsOf: url),
              let tiff = img.tiffRepresentation else { return }
        // Не помечаем skipNextChange — мы хотим, чтобы скрин попал в историю.
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setData(tiff, forType: .tiff)
    }
}
