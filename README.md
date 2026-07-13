<p align="center">
  <img src="src/parrot.png" width="96" alt="CopyPaster logo" />
</p>

<h1 align="center">CopyPaster</h1>

<p align="center">
  Clipboard history in the macOS menu bar.<br/>
  <code>⌥V</code> — pick a clip, it goes back into the window you came from.
</p>

<p align="center">
  <b>Screenshots land in the clipboard at once</b> — not five seconds later, while the floating thumbnail fades<br/>
  <b>Everything stays local</b> — the history lives on your disk, no cloud, no telemetry
</p>

## The history

`⌥V` — a card per clip. `⏎` or `1`…`9` pastes.

![The popup over the desktop](docs/screenshots/popup.png)

## Filter by app

The icon row on top. Step onto an app — the cards narrow to it.

![The app filter](docs/screenshots/filter.png)

## Search

Type. It filters from the first letter, matches marked.

![Live search](docs/screenshots/search.png)

## Keys

Up and down between zones, left and right inside one. The sheet lives in the icon menu.

![The shortcuts sheet](docs/screenshots/shortcuts.png)

## Install (macOS)

1. Download the DMG from the [Releases](https://github.com/olegperegudov/copypaster/releases/latest) page — `aarch64` for Apple Silicon, `x64` for Intel.
2. Drag CopyPaster into Applications and open it with **right click → Open** (the app is not notarized with Apple).
3. Grant **Accessibility** (System Settings → Privacy & Security → Accessibility). Without it the app cannot paste on your behalf.

Updates arrive on their own: the menu-bar icon gets a green dot and its menu offers "Update to vX.Y.Z".

## Instant screenshots

Shift-Cmd-4 saves a file instead of copying an image, and while the floating thumbnail hangs in the corner that file is not on disk yet — about five seconds during which "copy the screenshot" pastes the previous clip.

The menu item **"Screenshot straight to clipboard (no thumbnail)"** turns the thumbnail off. The file lands at once, CopyPaster catches it, and a plain `⌘V` pastes the screenshot.

## Development

```bash
npm install
npm run tauri dev                  # run it
npm test                           # search, filtering and highlighting tests
cd src-tauri && cargo test --lib   # history tests
```

Every push to `main` is a release: CI bumps the patch version itself, builds macOS (Apple Silicon and Intel separately) and Windows, then publishes the release and the auto-update manifest.

The current session log: `~/Library/Application Support/copypaster/debug.log`.

## Stack

Tauri 2 — Rust on the outside, HTML/CSS/JS on the inside, one codebase for macOS and Windows. The same rails as [Ribbit](https://github.com/olegperegudov/ribbit) and [Quill](https://github.com/olegperegudov/quill): a shared build, signing and update pipeline across all three apps.

## License

MIT.
