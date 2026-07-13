# Development

Everything a user does not need to know. The [README](../README.md) is the front door.

## Stack

[Tauri 2](https://tauri.app/) — Rust on the outside, plain HTML/CSS/JS on the inside, no bundler, one codebase for macOS and Windows. The same rails as [Ribbit](https://github.com/olegperegudov/ribbit) and [Quill](https://github.com/olegperegudov/quill): a shared build, signing and update pipeline across all three.

The popup is a non-activating `NSPanel` — it takes the keyboard but never takes focus from the app underneath, so a paste lands where you were working. The frontmost pid is remembered when the popup opens and the app is raised again before the keystroke goes out.

## Run it locally

```bash
npm install
npm run tauri dev
```

## Tests

```bash
npm test                           # frontend: search, filtering, highlighting, escaping
cd src-tauri && cargo test --lib   # backend: history, store, paste
```

Both run in CI before anything is built.

## Release

Every push to `main` is a release. CI bumps the patch version itself, tags it, builds Windows and both macOS architectures, and publishes the GitHub release plus the `latest.json` the in-app updater reads. Never bump by hand.

Each release also carries version-less copies of the installers (`CopyPaster_macOS_AppleSilicon.dmg`, `CopyPaster_macOS_Intel.dmg`, `CopyPaster_Windows_Setup.exe`) so the README buttons can link straight at a file that survives the next bump.

## Signing

macOS builds are signed with a stable self-signed certificate ("CopyPaster Code Signing"), not ad-hoc. macOS binds the Accessibility grant to the *signature*, so the user grants it once, at install, and updates never re-ask — an ad-hoc signature changes with every build and would.

Not notarized (that needs a paid Apple account), so the first open still needs `xattr -cr`.

## Synthetic keystrokes

⌘V is posted as a raw `CGEvent` on the **physical** V key (`kVK_ANSI_V` = 9) with the Command flag set on the event. Never address the key by its letter: a lookup through the active layout finds no "v" on a Cyrillic layout, falls through to keycode 0 — which is the A key — and the paste silently goes out as ⌘A. That shipped once; see the 0.1.15 entry in the [changelog](../CHANGELOG.md).

## Where things live

| | |
|---|---|
| History, images | `~/Library/Application Support/copypaster/` — `index.json` plus `img/<id>.png` |
| Session log | `~/Library/Application Support/copypaster/debug.log` |
