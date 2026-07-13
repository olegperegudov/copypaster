// Puts a password-manager-style clip on the pasteboard: the text, plus the
// marker every macOS password manager stamps on it. Used to check by hand that
// the watcher throws such a clip away instead of filing it into the history.
//
//   swift src-tauri/tests/stage_concealed_clip.swift          # concealed clip
//   swift src-tauri/tests/stage_concealed_clip.swift --plain  # ordinary clip
//
// then, in src-tauri:  cargo test --lib -- --ignored concealed

import AppKit

let concealed = !CommandLine.arguments.contains("--plain")
let pb = NSPasteboard.general
pb.clearContents()
pb.setString("hunter2", forType: .string)
if concealed {
    pb.setString("", forType: NSPasteboard.PasteboardType("org.nspasteboard.ConcealedType"))
}
print(concealed ? "staged: concealed clip" : "staged: ordinary clip")
