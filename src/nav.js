// Cursor arithmetic for the popup's two axes. Pure, so it is testable without a
// DOM: the zone machine in main.js reads like prose only if the index maths lives
// somewhere else.

/** Cards and the search caret stop at the ends. */
export function clamp(idx, len) {
  if (len === 0) return 0;
  return Math.min(Math.max(idx, 0), len - 1);
}

/** The app row is a ring: past the last app is the first one, and back. */
export function wrap(idx, len) {
  if (len === 0) return 0;
  return ((idx % len) + len) % len;
}

/** Where the cursor goes after the list under it changed shape.
 *
 *  The user is looking at a card, not at a position: a letter added to the search
 *  takes cards away around it, and the one they were reading has to stay under the
 *  cursor. Only when the search has taken that card away too is there nothing to
 *  hold on to, and the cursor goes back to the front. */
export function keepCursorOn(list, id) {
  const idx = list.findIndex((clip) => clip.id === id);
  return idx === -1 ? 0 : idx;
}
