<p align="center">
  <img src="src/parrot.png" width="96" alt="CopyPaster logo" />
</p>

<h1 align="center">CopyPaster</h1>

<p align="center">
  Clipboard history in the menu bar.<br/>
  <code>⌥V</code> — pick a clip, it goes straight back into the window you came from.
</p>

<p align="center">
  <b>Screenshots land on the clipboard at once</b> — not five seconds later, once the thumbnail fades<br/>
  <b>Everything stays local</b> — the history lives on your disk, no cloud, no telemetry
</p>

## Get it

<p align="center">
  <a href="https://github.com/olegperegudov/copypaster/releases/latest/download/CopyPaster_macOS_AppleSilicon.dmg"><img src="https://img.shields.io/badge/Download_for_macOS-Apple_Silicon-000?style=for-the-badge&logo=apple&logoColor=white" alt="Download for macOS, Apple Silicon" /></a>&nbsp;
  <a href="https://github.com/olegperegudov/copypaster/releases/latest/download/CopyPaster_macOS_Intel.dmg"><img src="https://img.shields.io/badge/Download_for_macOS-Intel-666?style=for-the-badge&logo=apple&logoColor=white" alt="Download for macOS, Intel" /></a>&nbsp;
  <a href="https://github.com/olegperegudov/copypaster/releases/latest/download/CopyPaster_Windows_Setup.exe"><img src="https://img.shields.io/badge/Download_for-Windows-0078D4?style=for-the-badge&logo=windows&logoColor=white" alt="Download for Windows" /></a>
</p>

Each button downloads the latest installer for that platform. Want an older build? Every version is on the [releases page](https://github.com/olegperegudov/copypaster/releases).

Then:

1. **Open it.** Apple isn't paid to trust us, so the first launch claims the app is *"damaged"*. It isn't — run `xattr -cr /Applications/CopyPaster.app` once in Terminal, then open it normally. Updates after that install themselves.
2. **Grant Accessibility** when asked (System Settings → Privacy & Security → Accessibility). Without it the app cannot paste on your behalf. Once, at install — not again after every update.
3. **Press ⌥V.** The history is there.

CopyPaster is built and used on macOS. The Windows build exists and installs, but it isn't tested nearly as much — expect rough edges.

## The history

`⌥V` — a card per clip. `⏎` or `1`…`9` pastes it back where you were.

![The popup over the desktop](docs/screenshots/popup.png)

## Filter by app

The icon row on top. Step onto an app — the cards narrow to it.

![The app filter](docs/screenshots/filter.png)

## Search

Just type. It filters from the first letter, matches marked.

![Live search](docs/screenshots/search.png)

## Keys

Up and down between zones, left and right inside one. The sheet lives in the menu-bar menu.

![The shortcuts sheet](docs/screenshots/shortcuts.png)

## Instant screenshots

Shift-Cmd-4 saves a file rather than copying an image, and while the little thumbnail hangs in the corner that file is not on disk yet — five seconds during which "copy the screenshot" pastes the previous clip.

Tick **"Screenshot straight to clipboard (no thumbnail)"** in the menu. The file lands at once, CopyPaster catches it, and a plain `⌘V` pastes the screenshot.

## Updates

The parrot in the menu bar turns green when a new version is out. Click it, pick the update line — done.

## Privacy

- Clips never leave the machine — no cloud, no sync, no telemetry.
- The history sits unencrypted in a folder on your disk, the same as Paste and the rest. A password that passes through the clipboard settles there too.

## Under the hood

Stack, local build, tests, signing and the release pipeline → [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## License

MIT
