# CopyPaster

Minimal clipboard manager for macOS — a Win+V analogue. Keeps the last 20 clipboard items (text + images), shows them in a popup picker on `⌥V`, pastes the selected one into the focused window.

Native Swift + SwiftUI, no dependencies, single binary.

## Features

- Background menu-bar app (no Dock icon).
- Polls `NSPasteboard` and remembers the last 20 items.
- `⌥V` opens the picker; arrow keys navigate, `Enter` / click pastes, `Esc` / outside-click dismisses.
- `⌘V` is left untouched — standard system paste still works.
- Screenshots taken via `⌘⇧3` / `⌘⇧4` are auto-copied to the clipboard (so they land in history and become immediately pasteable).
- Single Accessibility prompt on first launch (needed for synthesizing `⌘V` to the focused window).

## Build

Requires macOS 14+ and the Swift toolchain (Command Line Tools is enough).

```sh
./build.sh
```

Produces `CopyPaster.app` in the project root, ad-hoc signed.

## Run

```sh
open CopyPaster.app
```

On first launch macOS will ask for Accessibility — grant it (System Settings → Privacy & Security → Accessibility). Without it the auto-paste (synthetic `⌘V`) silently fails; everything else still works.

## Notes

- Ad-hoc signed only (no Apple Developer cert, no notarization). Gatekeeper may complain on first run; either right-click → Open, or `xattr -dr com.apple.quarantine CopyPaster.app`.
- Every rebuild changes the binary's `cdhash`, so macOS treats it as a new app and revokes the previous Accessibility grant. Reset via:

  ```sh
  tccutil reset Accessibility com.olegperegudov.copypaster
  ```

  then re-grant on next launch.
- History lives in memory only — not persisted between restarts.

## Hotkey

`⌥V` (Option + V) is registered via Carbon `RegisterEventHotKey` so it doesn't require an Input Monitoring permission and survives the popup grabbing key-window status.

## License

MIT.
