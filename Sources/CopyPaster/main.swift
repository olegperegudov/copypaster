// CopyPaster — minimal clipboard manager для macOS.
// Menu-bar app, polling NSPasteboard, Option+V → попап-список → Enter → авто-вставка Cmd+V.
// Скриншоты Cmd+Shift+3/4 (файл на Desktop) автоматически копируются в pasteboard.

import AppKit
import ApplicationServices
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem!
    private var watcher: PasteboardWatcher!
    private var screenshotWatcher: ScreenshotWatcher!
    private var hotkey: HotKeyMonitor!
    private var store: HistoryStore!
    private var popup: PopupWindow!
    private var previousApp: NSRunningApplication?

    func applicationDidFinishLaunching(_ notification: Notification) {
        Log.write("[app]", "didFinishLaunching")

        // Спрашиваем Accessibility сразу — иначе авто-вставка молча провалится.
        let prompt = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
        let trusted = AXIsProcessTrustedWithOptions(prompt)
        Log.write("[app]", "AXIsProcessTrusted=\(trusted)")

        store = HistoryStore()

        watcher = PasteboardWatcher(onNew: { [weak self] item in
            self?.store.add(item)
        })
        watcher.start()

        screenshotWatcher = ScreenshotWatcher(watcher: watcher)
        screenshotWatcher.start()

        popup = PopupWindow(
            store: store,
            onPick: { [weak self] item in
                guard let self else { return }
                // Фокус мы не крали (nonactivatingPanel) — целевое окно уже активно.
                // Paster внутри ждёт отпускания модификаторов и постит Cmd+V.
                Paster.paste(item, watcher: self.watcher)
            },
            onCancel: {
                // Esc — просто закрыть, фокус никогда не уходил.
            },
            onClickOutside: {
                // Клик по другому окну — фокус уйдёт туда естественно.
            }
        )

        hotkey = HotKeyMonitor(onPress: { [weak self] in
            guard let self else { return }
            self.previousApp = NSWorkspace.shared.frontmostApplication
            Log.write("[hotkey]", "Option+V fired, prevApp=\(self.previousApp?.localizedName ?? "nil")")
            self.popup.toggle()
        })
        hotkey.install()

        setupStatusItem()
    }

    private func setupStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "doc.on.clipboard", accessibilityDescription: "CopyPaster")
        }
        let menu = NSMenu()
        menu.addItem(NSMenuItem(title: "Open (⌥V)", action: #selector(openPopup), keyEquivalent: ""))
        menu.addItem(.separator())
        menu.addItem(NSMenuItem(title: "Quit CopyPaster", action: #selector(quit), keyEquivalent: "q"))
        statusItem.menu = menu
    }

    @objc private func openPopup() {
        previousApp = NSWorkspace.shared.frontmostApplication
        popup.toggle()
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let delegate = AppDelegate()
app.delegate = delegate
app.run()
