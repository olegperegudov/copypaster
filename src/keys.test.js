import { describe, expect, it } from "vitest";
import { keyAction } from "./keys.js";

const at = (over = {}) => ({ zone: "cards", query: "", hasFilter: false, ...over });
const press = (key, over = {}) => ({ key, ...over });

describe("typing goes to the search from wherever you are", () => {
  it("takes a letter on the cards", () => {
    expect(keyAction(press("a"), at())).toEqual({ type: "type", char: "a" });
  });

  it("takes a letter on the app row too", () => {
    expect(keyAction(press("s"), at({ zone: "apps" }))).toEqual({ type: "type", char: "s" });
  });

  it("takes Cyrillic, not just latin", () => {
    expect(keyAction(press("ж"), at())).toEqual({ type: "type", char: "ж" });
  });

  it("leaves ⌥-combinations alone — ⌥V is the key that opened the popup", () => {
    expect(keyAction(press("√", { alt: true }), at())).toBeNull();
  });

  it("leaves ⌘-combinations to the system", () => {
    expect(keyAction(press("q", { meta: true }), at())).toBeNull();
  });

  it("does not type a letter that arrived with ⌘ held", () => {
    expect(keyAction(press("a", { meta: true }), at({ query: "as" }))).toBeNull();
  });
});

describe("digits: a shortcut while the search is empty, characters once it is not", () => {
  it("pastes the third card", () => {
    expect(keyAction(press("3"), at())).toEqual({ type: "paste", index: 2 });
  });

  it("types the digit into a query being written", () => {
    expect(keyAction(press("2"), at({ query: "v" }))).toEqual({ type: "type", char: "2" });
  });
});

describe("backspace erases, delete deletes", () => {
  it("erases a letter of the query before anything else", () => {
    expect(keyAction(press("Backspace"), at({ query: "ass", hasFilter: true }))).toEqual({ type: "erase" });
  });

  it("clears the app filter once there is nothing left to erase", () => {
    expect(keyAction(press("Backspace"), at({ hasFilter: true }))).toEqual({ type: "clearFilter" });
  });

  it("does nothing with no query and no filter — it must never reach a card", () => {
    expect(keyAction(press("Backspace"), at())).toBeNull();
  });

  it("deletes the selected card", () => {
    expect(keyAction(press("Delete"), at())).toEqual({ type: "deleteCard" });
  });

  it("deletes on ⌘⌫ too — Fn+⌫ is not a one-handed gesture on a laptop", () => {
    expect(keyAction(press("Backspace", { meta: true }), at())).toEqual({ type: "deleteCard" });
  });

  it("⌘⌫ deletes the card, it does not erase the query", () => {
    expect(keyAction(press("Backspace", { meta: true }), at({ query: "ass" }))).toEqual({ type: "deleteCard" });
  });

  it("⌘⌫ on the app row deletes nothing", () => {
    expect(keyAction(press("Backspace", { meta: true }), at({ zone: "apps" }))).toBeNull();
  });

  it("does not delete a card from the app row, where nothing is selected", () => {
    expect(keyAction(press("Delete"), at({ zone: "apps" }))).toBeNull();
  });
});

describe("navigation is only ever between the cards and the icons", () => {
  it("goes up out of the cards", () => {
    expect(keyAction(press("ArrowUp"), at())).toEqual({ type: "zone", delta: -1 });
  });

  it("moves sideways inside the zone", () => {
    expect(keyAction(press("ArrowRight"), at())).toEqual({ type: "move", delta: 1 });
  });

  it("keeps moving sideways while a query is being typed — the search is not a stop", () => {
    expect(keyAction(press("ArrowLeft"), at({ query: "assist" }))).toEqual({ type: "move", delta: -1 });
  });

  it("pastes the selected card on Enter", () => {
    expect(keyAction(press("Enter"), at({ query: "assist" }))).toEqual({ type: "paste" });
  });

  it("closes on Escape", () => {
    expect(keyAction(press("Escape"), at())).toEqual({ type: "close" });
  });
});
