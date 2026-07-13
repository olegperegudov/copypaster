// Live search over the clipboard history.
//
// Matching is by word prefix, and the matched prefix is what gets highlighted —
// typing "ole" marks the first three letters of "Oleg". Any script counts as
// letters here, Cyrillic included. Same rule and same
// fuchsia mark as Ribbit's log search, so the two apps read alike.

/** Splits into words the way both apps do: letters and digits, any script. */
const WORD = /[\p{L}\p{N}]+/gu;

export function escapeHtml(s) {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** True when any word in the text starts with the query. */
export function matchesQuery(text, query) {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  const words = (text || "").toLowerCase().match(WORD) || [];
  return words.some((w) => w.startsWith(q));
}

/** Renders the text with every matching word's prefix wrapped in <mark>. */
export function highlightMatches(text, query) {
  const q = query.trim().toLowerCase();
  if (!q) return escapeHtml(text || "");

  let out = "";
  let last = 0;
  for (const m of (text || "").matchAll(WORD)) {
    const word = m[0];
    if (!word.toLowerCase().startsWith(q)) continue;
    out += escapeHtml(text.slice(last, m.index));
    out += `<mark class="hit">${escapeHtml(word.slice(0, q.length))}</mark>`;
    out += escapeHtml(word.slice(q.length));
    last = m.index + word.length;
  }
  out += escapeHtml(text.slice(last));
  return out;
}

/**
 * A clip matches the query through its text. An image has no text, so it only
 * survives an empty query — hiding images while searching is the honest answer:
 * we cannot tell whether a screenshot contains the word.
 */
export function clipMatches(clip, query) {
  if (!query.trim()) return true;
  if (clip.kind !== "text") return false;
  return matchesQuery(clip.text, query);
}

/** The cards to show: query first, then the app filter. */
export function visibleClips(clips, query, appBundle) {
  return clips
    .filter((c) => clipMatches(c, query))
    .filter((c) => !appBundle || c.appBundle === appBundle);
}

/**
 * The app row: one entry per app that still has clips *under the current query*,
 * with how many. Deliberately not filtered by the selected app — the row is how
 * you switch between apps, so it must keep showing the others.
 */
export function appRow(clips, query) {
  const byBundle = new Map();
  for (const c of clips) {
    if (!clipMatches(c, query)) continue;
    if (!c.appBundle) continue;
    const seen = byBundle.get(c.appBundle);
    if (seen) {
      seen.count += 1;
    } else {
      byBundle.set(c.appBundle, {
        bundle: c.appBundle,
        name: c.appName,
        icon: c.appIcon,
        count: 1,
      });
    }
  }
  return [...byBundle.values()];
}

/** "just now" / "4 min" / "2 h" / "3 d" — the card footer. */
export function age(createdAt, nowSecs) {
  const secs = Math.max(0, nowSecs - createdAt);
  if (secs < 45) return "just now";
  const mins = Math.round(secs / 60);
  if (mins < 60) return `${mins} min`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours} h`;
  return `${Math.round(hours / 24)} d`;
}
