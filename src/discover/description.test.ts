import { expect, test } from "vitest";
import { entryDescription } from "./description";

test("an entry's description is in the user's language when it has one", () => {
  const entry = { description: "Blissful.", translations: { de: { description: "Glückselig." } } };
  expect(entryDescription(entry, "de")).toBe("Glückselig.");
  expect(entryDescription(entry, "en")).toBe("Blissful.");
  expect(entryDescription({ description: "Blissful." }, "de")).toBe("Blissful.");
  expect(entryDescription({}, "de")).toBeNull();
});
