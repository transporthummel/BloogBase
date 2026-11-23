// The layout scripts/release.mjs relies on, checked with every push so that a
// typo in a heading fails CI instead of the release run.
import { describe, expect, it } from "vitest";
import markdown from "../../CHANGELOG.md?raw";
import news from "./changelog.json";

const CATEGORIES = ["Features", "Improvements", "Bug Fixes", "Misc"];
const headings = markdown.split(/\r?\n/).filter((line) => line.startsWith("#"));
const releases = headings.filter((h) => h.startsWith("## ") && h !== "## Unreleased");

describe("CHANGELOG.md", () => {
  it('starts with "# Changelog" and "## Unreleased"', () => {
    expect(headings.slice(0, 2)).toEqual(["# Changelog", "## Unreleased"]);
  });

  it("has only release headings like ## 1.2.3 (2026-10-27) after that", () => {
    const wrong = releases.filter((h) => !/^## \d+\.\d+\.\d+ \(\d{4}-\d{2}-\d{2}\)$/.test(h));
    expect(wrong).toEqual([]);
  });

  it("has only the four categories, written as release.mjs writes them", () => {
    const wrong = headings.filter(
      (h) => !h.startsWith("## ") && h !== "# Changelog" && !CATEGORIES.includes(h.slice(4)),
    );
    expect(wrong).toEqual([]);
  });

  it("lists the releases newest first", () => {
    const versions = releases.map((h) => h.split(" ")[1].split(".").map(Number));
    const sorted = [...versions].sort((a, b) => b[0] - a[0] || b[1] - a[1] || b[2] - a[2]);
    expect(versions).toEqual(sorted);
  });
});

describe("changelog.json", () => {
  it("has the same releases as CHANGELOG.md", () => {
    expect(news.releases.map((r) => r.version)).toEqual(releases.map((h) => h.split(" ")[1]));
  });

  it("has every entry in English and German", () => {
    for (const { en, de } of [news.unreleased, ...news.releases]) {
      expect(de).toHaveLength(en.length);
    }
  });
});
