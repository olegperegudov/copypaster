import { describe, expect, it } from "vitest";
import {
  age,
  appRow,
  clipMatches,
  highlightMatches,
  matchesQuery,
  visibleClips,
} from "./search.js";

const text = (id, s, bundle = "com.a", name = "A") => ({
  id,
  kind: "text",
  text: s,
  appBundle: bundle,
  appName: name,
  appIcon: "",
  createdAt: 0,
});
const image = (id, bundle = "system.screenshot") => ({
  id,
  kind: "image",
  text: "",
  appBundle: bundle,
  appName: "Снимок экрана",
  appIcon: "",
  createdAt: 0,
});

describe("matchesQuery", () => {
  it("matches by word prefix, not by substring", () => {
    expect(matchesQuery("Олег Перегудов", "оле")).toBe(true);
    // "лег" sits inside "Олег" but starts no word — a substring match would
    // light up half the history on every keystroke.
    expect(matchesQuery("Олег", "лег")).toBe(false);
  });

  it("matches any word, not just the first", () => {
    expect(matchesQuery("скинь отчёт по бенчу", "бен")).toBe(true);
  });

  it("is case-insensitive across scripts", () => {
    expect(matchesQuery("ALRAI-163", "alr")).toBe(true);
    expect(matchesQuery("scp report.pdf oleg@adv", "OLE")).toBe(true);
  });

  it("an empty query matches everything", () => {
    expect(matchesQuery("что угодно", "  ")).toBe(true);
  });
});

describe("highlightMatches", () => {
  it("marks the matched prefix only", () => {
    expect(highlightMatches("Олег", "оле")).toBe('<mark class="hit">Оле</mark>г');
  });

  it("marks every matching word", () => {
    const out = highlightMatches("Олег и Олеся", "оле");
    expect(out.match(/<mark/g)).toHaveLength(2);
  });

  it("leaves non-matching text alone", () => {
    expect(highlightMatches("привет", "оле")).toBe("привет");
  });

  it("escapes html so a copied tag cannot inject markup", () => {
    const out = highlightMatches('<img src=x onerror="boom">', "img");
    expect(out).not.toContain("<img");
    expect(out).toContain("&lt;");
  });

  it("escapes html around a highlight too", () => {
    const out = highlightMatches("<b>Олег</b>", "оле");
    expect(out).toContain('<mark class="hit">Оле</mark>');
    expect(out).toContain("&lt;b&gt;");
  });
});

describe("clipMatches", () => {
  it("keeps images when nothing is typed", () => {
    expect(clipMatches(image(1), "")).toBe(true);
  });

  it("drops images once a query is typed — their text is unknown", () => {
    expect(clipMatches(image(1), "оле")).toBe(false);
  });
});

describe("visibleClips", () => {
  const clips = [
    text(1, "Олег, посмотри п.7", "com.jira", "Jira"),
    text(2, "git push origin", "com.ghostty", "Ghostty"),
    text(3, "Олег Перегудов", "com.telegram", "Telegram"),
    image(4),
  ];

  it("query and app filter stack", () => {
    expect(visibleClips(clips, "оле", "com.jira").map((c) => c.id)).toEqual([1]);
  });

  it("app filter alone keeps everything from that app", () => {
    expect(visibleClips(clips, "", "com.ghostty").map((c) => c.id)).toEqual([2]);
  });

  it("no filters means the whole history", () => {
    expect(visibleClips(clips, "", null)).toHaveLength(4);
  });
});

describe("appRow", () => {
  const clips = [
    text(1, "Олег, посмотри", "com.jira", "Jira"),
    text(2, "git push", "com.ghostty", "Ghostty"),
    text(3, "Олег Перегудов", "com.telegram", "Telegram"),
    text(4, "Олег снова", "com.jira", "Jira"),
  ];

  it("counts clips per app", () => {
    const row = appRow(clips, "");
    expect(row.find((a) => a.bundle === "com.jira").count).toBe(2);
    expect(row).toHaveLength(3);
  });

  it("collapses to the apps that actually match the query", () => {
    const row = appRow(clips, "оле");
    expect(row.map((a) => a.bundle).sort()).toEqual(["com.jira", "com.telegram"]);
    expect(row.find((a) => a.bundle === "com.jira").count).toBe(2);
  });
});

describe("age", () => {
  it("reads as a human would say it", () => {
    expect(age(1000, 1010)).toBe("только что");
    expect(age(1000, 1000 + 4 * 60)).toBe("4 мин");
    expect(age(1000, 1000 + 2 * 3600)).toBe("2 ч");
    expect(age(1000, 1000 + 3 * 86400)).toBe("3 дн");
  });
});
