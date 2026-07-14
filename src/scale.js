//! One knob grows the whole interface.
//!
//! The size lives in settings; every window applies it here as a page zoom on
//! load, so the popup and both sheets render at the same factor. `zoom` scales
//! layout and type together, so the fixed pixel sizes (card widths, the strip's
//! paddings) grow in step with the text instead of the text spilling out of them.
//! The sheets' windows are grown to match in Rust (show_window), so the zoomed
//! content still fits inside the frame.

const { invoke } = window.__TAURI__.core;

export function applyScale(scale) {
  const s = Number(scale);
  document.documentElement.style.zoom = Number.isFinite(s) && s > 0 ? String(s) : "1";
}

export async function applyScaleFromSettings() {
  try {
    applyScale((await invoke("get_settings")).ui_scale);
  } catch {
    applyScale(1);
  }
}
