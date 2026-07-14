// What a keypress means in the popup. Pure, so the rules can be read — and
// tested — without a window: main.js only carries them out.
//
// The shape of it: there is nowhere to *go* to search. You walk the cards and the
// app icons, and whatever you type goes into the search on its own, from wherever
// you are standing. That is why ⌫ erases what you typed rather than deleting a
// card — the deleting key is ⌦.

/**
 * @param {{key: string, meta?: boolean, ctrl?: boolean, alt?: boolean}} press
 * @param {{zone: string, query: string, hasFilter: boolean}} state
 * @returns {object|null} the action, or null when the popup has no business with it
 */
export function keyAction(press, state) {
  const { key, meta, ctrl, alt } = press;
  const { zone, query, hasFilter } = state;

  // ⌘⌫ is how a Mac deletes the thing you are looking at — Finder, Mail, every
  // list. On a laptop the forward-delete key it stands in for is Fn+⌫, which is
  // not a one-handed gesture.
  if (meta && key === "Backspace") {
    return zone === "cards" ? { type: "deleteCard" } : null;
  }
  // Every other Cmd/Ctrl combination belongs to the system, not to us.
  if (meta || ctrl) return null;

  switch (key) {
    case "Escape":
      return { type: "close" };
    case "ArrowUp":
      return { type: "zone", delta: -1 };
    case "ArrowDown":
      return { type: "zone", delta: 1 };
    case "ArrowLeft":
      return { type: "move", delta: -1 };
    case "ArrowRight":
      return { type: "move", delta: 1 };
    case "Enter":
      return { type: "paste" };
    case "Backspace":
      // ⌫ takes back the last thing done *here*. Standing on the icons, that is
      // the filter they put on — never a letter of a query written elsewhere,
      // which is not even on screen next to the cursor. On the cards there is
      // nothing else it could mean: erase what was typed, and once there is
      // nothing left to erase, let the filter go.
      if (zone === "apps") return hasFilter ? { type: "clearFilter" } : null;
      if (query) return { type: "erase" };
      if (hasFilter) return { type: "clearFilter" };
      return null;
    case "Delete":
      // The other way in — a real ⌦ key, or Fn+⌫ on a laptop. Destructive either
      // way, so it only fires where the user is standing and can see what is
      // selected.
      return zone === "cards" ? { type: "deleteCard" } : null;
    default:
      break;
  }

  // A digit is a shortcut to the n-th card while the search is empty, and a
  // character the moment there is a query to add it to — you cannot search for
  // "v2" with a key that pastes card two.
  if (!query && /^[1-9]$/.test(key)) {
    return { type: "paste", index: Number(key) - 1 };
  }

  // Everything else printable is the search. Option-combinations are not: on a Mac
  // ⌥ turns letters into symbols, and ⌥V is the key that opened this window.
  if (key.length === 1 && !alt) {
    return { type: "type", char: key };
  }
  return null;
}
