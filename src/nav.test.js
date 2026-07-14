import { describe, expect, it } from "vitest";
import { clamp, keepCursorOn, wrap } from "./nav.js";

describe("clamp — cards stop at the ends", () => {
  it("holds at the last card", () => {
    expect(clamp(5, 3)).toBe(2);
  });

  it("holds at the first card", () => {
    expect(clamp(-1, 3)).toBe(0);
  });

  it("survives an empty list", () => {
    expect(clamp(2, 0)).toBe(0);
  });
});

describe("wrap — the app row is a ring", () => {
  it("steps right off the last app onto the first", () => {
    expect(wrap(5, 5)).toBe(0);
  });

  it("steps left off the first app onto the last", () => {
    expect(wrap(-1, 5)).toBe(4);
  });

  it("leaves an index inside the row alone", () => {
    expect(wrap(2, 5)).toBe(2);
  });

  it("survives an empty row", () => {
    expect(wrap(-1, 0)).toBe(0);
  });
});

describe("keepCursorOn — the cursor holds the card, not the position", () => {
  const list = (...ids) => ids.map((id) => ({ id }));

  it("follows the card when the search takes away the ones before it", () => {
    expect(keepCursorOn(list(7, 3), 3)).toBe(1);
  });

  it("follows it the other way too, when cards come back around it", () => {
    expect(keepCursorOn(list(9, 7, 3), 3)).toBe(2);
  });

  it("goes back to the front when the search excluded the card itself", () => {
    expect(keepCursorOn(list(9, 7), 3)).toBe(0);
  });

  it("goes to the front when there was no card to hold", () => {
    expect(keepCursorOn(list(9, 7), null)).toBe(0);
  });

  it("survives a search that matched nothing", () => {
    expect(keepCursorOn([], 3)).toBe(0);
  });
});
