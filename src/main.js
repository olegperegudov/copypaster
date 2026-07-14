// Popup behaviour: two zones, two axes.
//
// Up/down moves between the cards and the app icons above them; left/right moves
// inside whichever of the two holds the cursor. Each remembers where the cursor
// was, so stepping away and back does not lose your place.
//
// The search is deliberately not a third zone. Typing filters from wherever you
// are standing and the query surfaces above the icons, so the fast path is one
// gesture: ⌥V, type "assist", arrow to the card, Enter — with no trip up into a
// field and back down again.

import { clamp, keepCursorOn, wrap } from "./nav.js";
import { keyAction } from "./keys.js";
import { age, appRow, highlightMatches, visibleClips } from "./search.js";
import { applyScaleFromSettings } from "./scale.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

/** Top to bottom on screen, which is what up and down have to agree with. */
const ZONES = ["apps", "cards"];

const state = {
  clips: [],
  query: "",
  /** bundle id of the app being filtered on, or null for all apps. */
  appFilter: null,
  zone: "cards",
  cardIdx: 0,
  appIdx: 0,
  /** id of the card the cursor stood on before stepping up to the icons. Stepping
   *  up filters the cards under it away, so that card is not on screen to be
   *  remembered by position — but it is the one to come back to when the filter
   *  is let go. */
  leftCardId: null,
};

const el = {
  apps: document.getElementById("apps"),
  clear: document.getElementById("apps-clear"),
  search: document.getElementById("search"),
  query: document.getElementById("query"),
  count: document.getElementById("count"),
  cards: document.getElementById("cards"),
  empty: document.getElementById("empty"),
};

const log = (msg) => invoke("js_log", { message: msg }).catch(() => {});

// ---------- derived views ----------

const cards = () => visibleClips(state.clips, state.query, state.appFilter);
const apps = () => appRow(state.clips, state.query);

// ---------- rendering ----------

function render() {
  const list = cards();
  const row = apps();

  state.cardIdx = clamp(state.cardIdx, list.length);
  state.appIdx = clamp(state.appIdx, row.length);

  renderApps(row);
  renderSearch(list.length);
  renderCards(list);

  el.empty.hidden = state.clips.length > 0;
  el.apps.classList.toggle("focus", state.zone === "apps");
}

function renderApps(row) {
  el.clear.hidden = !state.appFilter;
  el.apps.replaceChildren();
  row.forEach((app, idx) => {
    const node = document.createElement("span");
    node.className = "app";
    if (app.bundle === state.appFilter) node.classList.add("on");
    if (state.zone === "apps" && idx === state.appIdx) node.classList.add("cursor");
    node.append(appIcon(app), document.createTextNode(app.name));
    const count = document.createElement("span");
    count.className = "count";
    count.textContent = String(app.count);
    node.append(count);
    node.addEventListener("click", () => {
      state.appIdx = idx;
      // Clicking the app you are already filtering on lets it go — otherwise the
      // mouse can set a filter it cannot clear.
      state.appFilter = state.appFilter === app.bundle ? null : app.bundle;
      state.cardIdx = 0;
      render();
    });
    el.apps.append(node);
  });
}

function appIcon(app) {
  if (app.icon) {
    const img = document.createElement("img");
    img.className = "app-icon";
    img.src = app.icon;
    img.alt = "";
    return img;
  }
  // No icon (a screenshot, or Windows): the app's initial keeps the header
  // aligned instead of leaving a hole.
  const stub = document.createElement("span");
  stub.className = "app-icon";
  stub.textContent = (app.name || "?").slice(0, 1).toUpperCase();
  return stub;
}

function renderSearch(visible) {
  // No query, no bar: an empty search box is a thing to look at and reason about,
  // and there is nothing to reason about until the user has typed something.
  el.search.hidden = state.query === "";
  el.query.textContent = state.query;
  el.count.textContent =
    state.query.trim() && state.clips.length
      ? `${visible} of ${state.clips.length}`
      : "";
}

function renderCards(list) {
  el.cards.replaceChildren();
  list.forEach((clip, idx) => {
    const card = document.createElement("div");
    card.className = "card";
    if (idx === state.cardIdx) card.classList.add("sel");

    const head = document.createElement("div");
    head.className = "card-head";
    head.append(appIcon({ name: clip.appName, icon: clip.appIcon }), document.createTextNode(clip.appName || "—"));

    const body = document.createElement("div");
    body.className = "card-body";
    if (clip.kind === "image") {
      body.classList.add("image");
      const img = document.createElement("img");
      img.src = clip.preview;
      img.alt = "";
      body.append(img);
    } else {
      // Trusted: highlightMatches escapes the clip text before marking it.
      body.innerHTML = highlightMatches(clip.text, state.query);
    }

    const foot = document.createElement("div");
    foot.className = "card-foot";
    const badge = document.createElement("span");
    badge.className = clip.kind === "image" ? "badge image" : "badge";
    badge.textContent = clip.kind === "image" ? "image" : "text";
    const meta = document.createElement("span");
    meta.textContent =
      clip.kind === "image"
        ? `${clip.width}×${clip.height}`
        : age(clip.createdAt, Math.floor(Date.now() / 1000));
    foot.append(badge, meta);

    card.append(head, body, foot);
    card.addEventListener("click", () => pick(clip.id));
    el.cards.append(card);
  });

  const selected = el.cards.children[state.cardIdx];
  if (selected) selected.scrollIntoView({ block: "nearest", inline: "nearest" });
}

// ---------- actions ----------

async function load() {
  state.clips = await invoke("get_history");
  // The count goes to the app log on purpose: an empty popup over a full
  // history is exactly what a missing event permission looks like, and from the
  // outside the two are indistinguishable.
  log(`history loaded: ${state.clips.length}`);
  render();
}

async function pick(id) {
  await invoke("pick", { id });
}

/** ⌦ on a card. The cursor stays where it stood, so holding the key walks the
 *  history away card by card instead of jumping to the end. */
async function drop(id) {
  await invoke("delete_clip", { id });
  await load();
  state.cardIdx = clamp(state.cardIdx, cards().length);
  render();
}

function setZone(zone) {
  if (state.zone === zone) return;
  state.zone = zone;
  if (zone === "apps") {
    state.leftCardId = cards()[state.cardIdx]?.id ?? null;
    // Stepping into the row is already a choice: the app under the cursor starts
    // filtering right away, and the ⌫ chip appears with it. Waiting for a
    // sideways press would mean standing on an app that is not the one you see
    // the cards for.
    state.appFilter = apps()[state.appIdx]?.bundle ?? null;
    state.cardIdx = 0;
  }
  invoke("set_zone", { zone }).catch(() => {});
  render();
}

function stepZone(delta) {
  const idx = ZONES.indexOf(state.zone);
  const next = idx + delta;
  if (next < 0 || next >= ZONES.length) return;
  setZone(ZONES[next]);
}

/** ⌫ on the icons, ⌫ on the cards with nothing left to erase, or a click on the
 *  chip. Dropping a filter only brings cards back, so the card to come back to is
 *  always there: on the icons it is the one left behind on the way up, on the
 *  cards the one under the cursor. */
function clearAppFilter() {
  const held = state.zone === "apps" ? state.leftCardId : cards()[state.cardIdx]?.id ?? null;
  state.appFilter = null;
  state.cardIdx = keepCursorOn(cards(), held);
  render();
}

/** Every route into the query runs through here — a filter on an app that the
 *  new query has emptied out of the row is one the user can no longer see or
 *  reach, so it goes rather than silently hiding cards.
 *
 *  The card under the cursor is held across the change: typing is done while
 *  reading the cards, and a card that survives the new query must not slide out
 *  from under the cursor because the ones before it disappeared. */
function setQuery(query) {
  const held = cards()[state.cardIdx]?.id ?? null;
  state.query = query;
  if (state.appFilter && !apps().some((a) => a.bundle === state.appFilter)) {
    state.appFilter = null;
  }
  state.cardIdx = keepCursorOn(cards(), held);
  render();
}

function moveCursor(delta) {
  if (state.zone === "cards") {
    state.cardIdx = clamp(state.cardIdx + delta, cards().length);
  } else if (state.zone === "apps") {
    const row = apps();
    // The row wraps: the apps are few and it is a ring, so holding one direction
    // walks the whole thing instead of parking you against an end.
    state.appIdx = wrap(state.appIdx + delta, row.length);
    // The filter follows the cursor: one arrow press is the whole gesture, no
    // Enter to confirm.
    state.appFilter = row[state.appIdx]?.bundle ?? null;
    state.cardIdx = 0;
  }
  render();
}

// ---------- keyboard ----------

document.addEventListener("keydown", (e) => {
  const action = keyAction(
    { key: e.key, meta: e.metaKey, ctrl: e.ctrlKey, alt: e.altKey },
    { zone: state.zone, query: state.query, hasFilter: !!state.appFilter }
  );
  if (!action) return;
  e.preventDefault();

  switch (action.type) {
    case "close":
      invoke("close_popup");
      return;
    case "zone":
      stepZone(action.delta);
      return;
    case "move":
      moveCursor(action.delta);
      return;
    case "paste": {
      // With an index it is a digit shortcut, without one it is the selected card.
      const clip = cards()[action.index ?? state.cardIdx];
      if (clip) pick(clip.id);
      return;
    }
    case "deleteCard": {
      const clip = cards()[state.cardIdx];
      if (clip) drop(clip.id);
      return;
    }
    case "type":
      setQuery(state.query + action.char);
      return;
    case "erase":
      setQuery(state.query.slice(0, -1));
      return;
    case "clearFilter":
      clearAppFilter();
      return;
  }
});

el.clear.addEventListener("click", clearAppFilter);

// The popup window is a full-width strip, mostly bare: what the user sees through
// it is their own screen, so a click on the bare part is a click *past* CopyPaster
// and means "go away". Clicks outside the window are caught natively (mac_window).
document.addEventListener("mousedown", (e) => {
  if (!e.target.closest(".glass, .card, .empty, .apps-clear")) invoke("close_popup");
});

// ---------- wiring ----------

listen("history-changed", load);

listen("popup-opened", async () => {
  // The scale may have changed in settings while the popup was hidden — pick it
  // up on the way in, before anything is drawn at the old size.
  applyScaleFromSettings();
  // A fresh summon starts clean: last time's query and filter are not what the
  // user means by "show me my clipboard".
  state.query = "";
  state.appFilter = null;
  state.zone = "cards";
  state.cardIdx = 0;
  state.appIdx = 0;
  state.leftCardId = null;
  invoke("set_zone", { zone: "cards" }).catch(() => {});
  await load();
});

applyScaleFromSettings();
load().then(() => log("popup ready"));
