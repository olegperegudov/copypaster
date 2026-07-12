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
