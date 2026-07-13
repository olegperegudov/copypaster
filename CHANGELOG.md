# Changelog

Engineering release notes. Primary reader: future Claude. Detailed on purpose —
enough to understand *what* changed and *why* without digging through diffs.

## Unreleased

**Pasting works on a Russian keyboard layout.** Picking a card put the clip on the clipboard and then pasted nothing — silently, every time, as long as a Cyrillic layout was active.

The paste was synthesised as ⌘ + `Key::Unicode('v')`. enigo resolves that letter through the *active layout*: it walks keycodes 0–127 asking each what character it types, and takes the one that answers "v". On a Russian layout no key answers "v", the search falls through with `pressed_keycode = 0` — and keycode 0 is the A key. So the app sent **⌘A**. Nothing pasted, and the target app quietly selected all of its text instead. On ABC the same code found the V key on 9 and worked, which is why this looked intermittent rather than broken.

Measured on the machine rather than guessed: `UCKeyTranslate` over both installed layouts — *Russian – PC* has no key producing "v" (key 9 types `м`), *ABC* has it on key 9.

- ⌘V now goes out as a raw key event on the **physical** V key (`kVK_ANSI_V` = 9) with `CGEventFlagCommand` set on the event itself. The receiving app reads the chord from the event's flags, so it holds no matter which layout is active or which modifiers are physically down. Same synthesis Quill already uses for ⌘C — it hit this trap first, in terminals and Electron apps.
- Windows keeps the enigo path (`Ctrl` + Unicode `v`). It has the same layout blind spot in principle; noted, not touched blind.

Tests: the paste key is asserted to be the hardware V and explicitly *not* keycode 0 — the exact value that shipped. Note the limit: the keystroke itself crosses into another app, which no unit test here can observe.

## 0.1.14 — 2026-07-13

**Backspace deletes the card you are standing on.** The key already meant "take this away" in the other two zones — it clears the app filter, and it deletes a character in search. On the cards it did nothing, and a clip you no longer wanted could only be waited out of the ring.

- `delete_clip` drops the item and writes the index straight away, so the clip does not come back after a restart. An image clip takes its file with it: `store::save` already sweeps whatever no longer has a clip behind it.
- A stale press is harmless: an id the history has already lost returns an error instead of taking the neighbouring card with it.
- The cursor stays put, so the next card slides under it — holding the key walks the history away one card at a time.

Tests: the clip leaves the history, a second delete of the same id takes no neighbour, the image file leaves the disk. Driven end to end in a browser against the real popup code: pressed `⌫` on the middle card, the card went and the cursor kept its place.

**The icon lost its box.** The tray, Dock and DMG icons are regenerated from a transparent parrot — the way Ribbit carries its frog. The old plate with the "COPY PASTER" caption is gone.

## 0.1.9 — 2026-07-13

**The history survives a restart.** Was: clips lived in memory only — any restart, an update above all, wiped them. Now: the history sits on disk and comes back at launch.

- `index.json` — everything light (text, source app, timestamp), rewritten whole on every new clip.
- `img/<id>.png` — images as separate files: written once, instead of being rewritten from scratch every time a word is copied. The file of a clip that fell off the ring is deleted, so the history does not leak onto the disk.
- The index is written to a temp file and renamed over the old one: a half-written index is worse than a stale one, because it is what the next launch reads.
- A corrupt index means an empty history, not a crash. Clip numbering continues from the last restored id — otherwise a new clip would take someone else's number and its card would paste the wrong thing.

Tests: the round trip "saved → restored → the card hands back the same bytes", numbering continuity, cleanup of the file behind a dropped clip, corrupt index.

Clips now sit on disk in the clear (`~/Library/Application Support/copypaster/`) — the same as Paste. Passwords that pass through the clipboard settle there too.

## 0.1.8 — 2026-07-13

**The app row is a ring.** Left from the first icon lands on the last one, right from the last one lands on the first. There are few apps, so hitting a wall buys nothing: one direction walks the whole row.

**The `⌫` chip no longer shoves the icons.** Was: it stood as the first element of the row and pushed every icon sideways the moment it appeared — a jerk on every filter. Now: the chip hangs as a tab above the row, in space that is reserved whether it shows up or not, so the icons stay put.

## 0.1.7 — 2026-07-13

**The popup takes the keyboard for itself.** Was: the panel accepted keys, but the app underneath stayed active — and everything the popup did not handle itself went through to it. Esc over an open popup closed the Telegram window, not the popup. Now: while the popup is open, it is the active one; when it leaves — by Esc, by the hotkey or after a paste — the keyboard goes back to the app it was called from (by the remembered pid, exactly the way pasting already did it). There are exactly three actions over the popup: pick a card, Esc, click away.

A non-activating panel (the Spotlight mechanism) was chosen so that the paste would land in the original app. But we remember the pid anyway and hand focus back explicitly, so "never activate at all" bought nothing and leaked keys.

**Stepping up to the app row turns the filter on at once.** Was: you moved onto the icons and nothing happened — you had to press sideways as well, and only then did the "⌫ clear" chip appear. Now: a step up onto the icons is already a choice — the app under the cursor filters the cards, and the chip is visible right away.

## 0.1.6 — 2026-07-13

**The shortcuts sheet is a normal window.** Was: a frameless window on top of everything else, closable only by clicking the menu item again — it covered whatever opened after it. Now: a system frame with a close button, `⌘W` and Esc close it, and it does not float above other windows. Closing hides the window instead of destroying it, otherwise the menu item could not open it again.

**A menu item named after the result.** Was: "Instant screenshots" — the name spoke about the mechanism, not the outcome. Now: "Screenshot straight to clipboard (no thumbnail)" — ticking it removes the macOS floating thumbnail, and the capture reaches the clipboard at once instead of five seconds later.

## 0.1.4 — 2026-07-12

Fixes from the first live run.

**The elements are opaque.** Was: cards, search and the app row were see-through (72% opacity + background blur) — over a busy wallpaper the text was hard to read. Now: a solid dark background. There is still no shared backdrop under the popup — the zones keep floating over the screen, but each of them now honestly covers what is beneath it.

**A click away closes the popup** — not only Esc.
- Clicks outside the window are caught by a global mouse monitor (`NSEvent`): a non-activating panel never becomes the active app, so it never gets a "lost focus" event and there is nothing else to hang the closing on. The monitor does not see clicks inside our own window — picking a card does not close the popup from under itself. Such a monitor needs no Accessibility permission (that is only for keyboard ones).
- Clicks on the empty part of the window are caught by the popup itself: the window is a full-width strip and the desktop shows through it, so a click on empty space is a click away.
- On Windows the panel is a normal window, where focus loss fires.

**The `⌫` key is visible when there is something to clear.** Was: the app filter went on with an arrow key, and only a reader of the shortcuts sheet knew how to take it off. Now: while the filter is on, a "⌫ clear" chip sits at the left of the app row — it names the key and works as a button.

## 0.1.0 — 2026-07-12

A full rebuild: Swift/SwiftUI → Tauri 2 (Rust + HTML/CSS/JS).

### Why the stack changed

Updates, signing and CI for Ribbit and Quill are built on Tauri: a `latest.json` manifest in GitHub Releases, a minisign signature, a background check every 30 minutes, an auto version bump on every push to `main`. A Swift app cannot reuse that pipeline — it would need a second, separate one (Sparkle plus its own workflow). The app was small (~680 lines, half of them UI that was being rewritten anyway), so moving it onto the shared rails was cheaper than keeping two schemes alive. A Windows build came along for free.

### What it became

**Instant screenshots.** Was: a screenshot reached the clipboard in ~6.5 s, and `⌘V` right after Shift-Cmd-4 pasted the previous clip. Now: ~1.5 s, and with the floating thumbnail off — immediately.
- The macOS floating thumbnail (~5 s) is the bulk of the delay: while it hangs there, the file is not on disk yet. The "Instant screenshots" menu item turns it off.
- The screenshots folder is watched through filesystem events instead of a poll once a second (−1 s).
- A screenshot goes straight into the history, skipping the detour "put it on the clipboard → wait for our own clipboard poll" (−0.5 s).

**A popup instead of a list of lines.** Was: a 280×280 vertical list, one truncated line per clip, images squashed into 80×40. Now: three elements floating over the screen with no shared backdrop — a carousel of cards, a search field, an app row.
- A card shows the content, the source app with its icon, the kind and the age of the clip.
- Live search: filters from the first letter, matches highlighted in fuchsia (`#c25cce` — the same mark and the same "match at the start of a word" rule as Ribbit's log search).
- App filter: the arrow lands on an app and the cards narrow, no Enter needed. `⌫` clears the filter and takes focus down into search.
- Navigation on two axes: up and down between zones, left and right inside a zone. Each zone remembers where the cursor stood.
- The popup is a non-activating NSPanel: it accepts the keyboard but does not pull focus from the app underneath, so the paste lands where you were working.

**A menu-bar menu.** Check for updates (once one is found — "Update to vX.Y.Z", and the icon turns green), Shortcuts, Instant screenshots, the version, quit.

**A shortcuts sheet** — a separate window that highlights the zone you are standing in: the same key does different things in different zones (digits pick a card — or get typed into search; `⌫` deletes a character — or clears the filter).

**Signing.** A stable self-signed certificate, "CopyPaster Code Signing" (as in Ribbit): the Accessibility permission binds to the certificate rather than to the build hash, and survives updates. An ad-hoc signature would make the user grant it again after every release.

### Tests

- Rust: the history — order, dedup of consecutive duplicates, size limit, preview truncation.
- JS: word-prefix search, highlighting, HTML escaping (a copied `<img onerror=…>` must not run inside a card), filters and their combination, the counters in the app row.
