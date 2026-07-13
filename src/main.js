// Popup behaviour: three zones, two axes.
//
// Up/down moves between zones (cards → search → apps, bottom-up); left/right
// moves inside the zone that holds the cursor. Each zone remembers where the
// cursor was, so stepping away and back does not lose your place.

import { clamp, wrap } from "./nav.js";
import { age, appRow, highlightMatches, visibleClips } from "./search.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const ZONES = ["apps", "search", "cards"];

const state = {
  clips: [],
  query: "",
  /** bundle id of the app being filtered on, or null for all apps. */
  appFilter: null,
  zone: "cards",
  cardIdx: 0,
  appIdx: 0,
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
  el.search.classList.toggle("focus", state.zone === "search");
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
  el.count.textContent =
    state.query.trim() && state.clips.length
      ? `${visible} of ${state.clips.length}`
      : "";
  if (el.query.value !== state.query) el.query.value = state.query;
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

function setZone(zone) {
  if (state.zone === zone) return;
  state.zone = zone;
  if (zone === "apps") {
    // Stepping into the row is already a choice: the app under the cursor starts
    // filtering right away, and the ⌫ chip appears with it. Waiting for a
    // sideways press would mean standing on an app that is not the one you see
    // the cards for.
    state.appFilter = apps()[state.appIdx]?.bundle ?? null;
    state.cardIdx = 0;
  }
  if (zone === "search") {
    el.query.focus();
  } else {
    el.query.blur();
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

/** Backspace in the app row: drop the filter and fall through to the search. */
function clearAppFilter() {
  state.appFilter = null;
  state.cardIdx = 0;
  setZone("search");
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
  const inSearch = state.zone === "search";

  switch (e.key) {
    case "Escape":
      e.preventDefault();
      invoke("close_popup");
      return;

    case "ArrowUp":
      e.preventDefault();
      stepZone(-1);
      return;

    case "ArrowDown":
      e.preventDefault();
      stepZone(1);
      return;

    case "ArrowLeft":
    case "ArrowRight":
      // In the search field the arrows belong to the caret, as in any text box.
      if (inSearch) return;
      e.preventDefault();
      moveCursor(e.key === "ArrowLeft" ? -1 : 1);
      return;

    case "Enter": {
      e.preventDefault();
      const list = cards();
      const clip = list[state.cardIdx];
      if (clip) pick(clip.id);
      return;
    }

    case "Backspace":
      // In the app row it clears the filter; in the search field it is just
      // backspace, deleting a character.
      if (state.zone === "apps") {
        e.preventDefault();
        clearAppFilter();
      }
      return;

    default:
      break;
  }

  // Digits pick the n-th card — but only where they are not text. Typing "1"
  // into the search field must produce a "1".
  if (!inSearch && /^[1-9]$/.test(e.key)) {
    e.preventDefault();
    const clip = cards()[Number(e.key) - 1];
    if (clip) pick(clip.id);
    return;
  }

  // Any printable character starts a search from wherever you are — the fastest
  // path to a clip you can name.
  if (!inSearch && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
    setZone("search");
  }
});

el.query.addEventListener("input", () => {
  state.query = el.query.value;
  // A query that empties the current app out of the row leaves a filter the
  // user can no longer see or reach — drop it rather than silently hiding cards.
  if (state.appFilter && !apps().some((a) => a.bundle === state.appFilter)) {
    state.appFilter = null;
  }
  state.cardIdx = 0;
  render();
});

el.search.addEventListener("click", () => setZone("search"));
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
  // A fresh summon starts clean: last time's query and filter are not what the
  // user means by "show me my clipboard".
  state.query = "";
  state.appFilter = null;
  state.zone = "cards";
  state.cardIdx = 0;
  state.appIdx = 0;
  el.query.value = "";
  el.query.blur();
  invoke("set_zone", { zone: "cards" }).catch(() => {});
  await load();
});

load().then(() => log("popup ready"));
