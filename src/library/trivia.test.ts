import { expect, test } from "vitest";
import { pickTrivia, trivia, triviaText, triviaTranslations } from "./trivia";

test("the fact files have no repeated lines", () => {
  expect(trivia.length).toBeGreaterThan(0);
  for (const facts of [trivia, ...Object.values(triviaTranslations)])
    expect(new Set(facts).size).toBe(facts.length);
});

test("every translation has a line for each fact", () => {
  for (const facts of Object.values(triviaTranslations)) expect(facts).toHaveLength(trivia.length);
});

test("shows a fact in the app language", () => {
  expect(triviaText(trivia[0], "de")).toBe(triviaTranslations.de[0]);
  expect(triviaText(trivia[0], "en")).toBe(trivia[0]);
  expect(triviaText("Not in the file.", "de")).toBe("Not in the file.");
});

test("picks only facts that weren't shown yet", () => {
  const picked = pickTrivia(["a", "b", "c"], ["a", "c"], () => 0.99);
  expect(picked).toEqual({ fact: "b", seen: ["a", "c", "b"] });
});

test("forgets facts that are no longer in the file", () => {
  expect(pickTrivia(["a", "b"], ["gone", "a"], () => 0)?.seen).toEqual(["a", "b"]);
});

test("starts a new round without repeating the last fact", () => {
  for (const r of [0, 0.5, 0.99]) {
    const picked = pickTrivia(["a", "b", "c"], ["a", "b", "c"], () => r);
    expect(picked?.fact).not.toBe("c");
    expect(picked?.seen).toEqual(["c", picked?.fact]);
  }
  expect(pickTrivia(["a"], ["a"])?.fact).toBe("a");
  expect(pickTrivia([], [])).toBeNull();
});
