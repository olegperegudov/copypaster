// The cheat sheet follows the popup: the same key does different things per
// zone (digits pick a card, or type a digit), so the zone you are standing in is
// the one lit up.

import { applyScaleFromSettings } from "./scale.js";

const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;

applyScaleFromSettings();

const zones = document.querySelectorAll(".zone[data-zone]");

function highlight(zone) {
  zones.forEach((node) => {
    node.classList.toggle("active", node.dataset.zone === zone);
  });
}

listen("zone-changed", (event) => highlight(event.payload));

// The popup is gone — no zone is live, so nothing is lit.
listen("popup-closed", () => highlight(null));

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") getCurrentWindow().hide();
});
