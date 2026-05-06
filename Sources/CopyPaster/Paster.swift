// Paster: кладёт элемент на NSPasteboard и постит Cmd+V в активное окно.
// CGEventPost требует Accessibility — диалог запрашивается один раз при первом использовании.

import AppKit
import Carbon.HIToolbox

enum Paster {
    static func paste(_ item: ClipItem, watcher: PasteboardWatcher) {
        let kind: String
        switch item.kind {
        case .text(let s): kind = "text(\(s.prefix(20))…)"
        case .image: kind = "image"
        }
        Log.write("[paste]", "called for \(kind)")

        watcher.skipNextChange = true

        let pb = NSPasteboard.general
        pb.clearContents()
        switch item.kind {
        case .text(let s):
            pb.setString(s, forType: .string)
        case .image(let img):
            if let tiff = img.tiffRepresentation {
                pb.setData(tiff, forType: .tiff)
            }
        }

        DispatchQueue.global(qos: .userInteractive).async {
            var waited = 0
            for _ in 0..<30 {
                let active: NSEvent.ModifierFlags = [.option, .command, .shift, .control]
                if NSEvent.modifierFlags.intersection(active).isEmpty { break }
                usleep(10_000)
                waited += 10
            }
            Log.write("[paste]", "modifier wait \(waited)ms, flags=\(NSEvent.modifierFlags.rawValue)")
            usleep(40_000)
            DispatchQueue.main.async {
                let front = NSWorkspace.shared.frontmostApplication?.localizedName ?? "nil"
                Log.write("[paste]", "sendCommandV → frontApp=\(front)")
                sendCommandV()
            }
        }
    }

    private static func sendCommandV() {
        let src = CGEventSource(stateID: .combinedSessionState)
        let cmd = CGKeyCode(kVK_Command)
        let v = CGKeyCode(kVK_ANSI_V)

        // Полная последовательность: Cmd↓ → V↓ → V↑ → Cmd↑.
        // Надёжнее чем просто V с флагом Command — некоторые apps не реагируют.
        let cmdDown = CGEvent(keyboardEventSource: src, virtualKey: cmd, keyDown: true)
        let vDown = CGEvent(keyboardEventSource: src, virtualKey: v, keyDown: true)
        let vUp = CGEvent(keyboardEventSource: src, virtualKey: v, keyDown: false)
        let cmdUp = CGEvent(keyboardEventSource: src, virtualKey: cmd, keyDown: false)

        vDown?.flags = .maskCommand
        vUp?.flags = .maskCommand

        cmdDown?.post(tap: .cghidEventTap)
        vDown?.post(tap: .cghidEventTap)
        vUp?.post(tap: .cghidEventTap)
        cmdUp?.post(tap: .cghidEventTap)
    }
}
