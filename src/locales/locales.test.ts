import { expect, test } from "vitest";
import de from "./de.json";
import en from "./en.json";

function keys(obj: object, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    typeof v === "object" ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`],
  );
}

test("German and English have the same keys", () => {
  expect(keys(de).sort()).toEqual(keys(en).sort());
});
