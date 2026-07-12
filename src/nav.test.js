import { describe, expect, it } from "vitest";
import { clamp, wrap } from "./nav.js";

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
