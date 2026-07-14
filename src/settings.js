// The settings sheet: what the user chose, nothing the app can work out itself.
//
// Retention is the important one. A clipboard history with no expiry is a
// transcript of everything you ever copied, so the app asks how long you want to
// keep it — and shortening the window deletes what already fell outside it,
// immediately, not at some later sweep.

import { applyScale } from "./scale.js";

const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;

const $ = (sel) => document.querySelector(sel);

const LABEL = { 0: "No limit", 1: "1 day", 7: "7 days", 30: "30 days" };

let statusTimer = null;
function say(text) {
  $("#status").textContent = text;
  clearTimeout(statusTimer);
  statusTimer = setTimeout(() => ($("#status").textContent = ""), 3000);
}

function renderRetention(choices, chosen) {
  const box = $("#retention");
  box.innerHTML = "";
  for (const days of choices) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "choice";
    b.textContent = LABEL[days] ?? `${days} days`;
    b.classList.toggle("chosen", days === chosen);
    b.addEventListener("click", async () => {
      try {
        await invoke("set_retention_days", { days });
        renderRetention(choices, days);
        say(days === 0 ? "Clips are kept until the list pushes them out." : `Clips older than ${LABEL[days]} are gone.`);
      } catch (err) {
        say(`Couldn't save: ${err}`);
      }
    });
    box.appendChild(b);
  }
}

// This window opens at the saved scale; the sample line nets the *target* scale
// on top of that, so a drag previews the new size without resizing the window
// under the cursor. The popup and the cheat sheet take the change on next open.
let openedScale = 1;

function reflectScale(target) {
  $("#scale-value").textContent = `${Math.round(target * 100)}%`;
  $("#scale-sample").style.zoom = String(target / openedScale);
}

function renderScale(cfg) {
  openedScale = cfg.ui_scale ?? 1;
  const slider = $("#ui-scale");
  // The ends come from Rust, which owns the range and clamps to it — the slider
  // is just its face, so it must not carry a second copy of the bounds.
  slider.min = String(cfg.ui_scale_min);
  slider.max = String(cfg.ui_scale_max);
  slider.step = "0.05";
  slider.value = String(openedScale);
  reflectScale(openedScale);

  slider.addEventListener("input", () => reflectScale(Number(slider.value)));
  // Persist once, when the drag settles — not on every intermediate pixel.
  slider.addEventListener("change", async () => {
    try {
      await invoke("set_ui_scale", { scale: Number(slider.value) });
      say("Saved — the popup uses it the next time you open it.");
    } catch (err) {
      say(`Couldn't save: ${err}`);
    }
  });
}

async function load() {
  const cfg = await invoke("get_settings");
  applyScale(cfg.ui_scale);
  renderRetention(cfg.retention_choices, cfg.retention_days);
  $("#instant").checked = cfg.instant_screenshots;
  renderScale(cfg);
}

$("#instant").addEventListener("change", async (e) => {
  try {
    await invoke("set_instant_screenshots", { on: e.target.checked });
  } catch (err) {
    e.target.checked = !e.target.checked;
    say(`Couldn't change it: ${err}`);
  }
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") getCurrentWindow().hide();
});

load();
