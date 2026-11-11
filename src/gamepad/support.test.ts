import { describe, expect, it } from "vitest";
import { describeGamepadSupport } from "./support";

describe("describeGamepadSupport", () => {
  it("says the API is there when getGamepads answers", () => {
    expect(describeGamepadSupport({ getGamepads: () => [null] }, true)).toBe(
      "Gamepad API available (secure context: yes)",
    );
  });

  it("says it's missing without getGamepads", () => {
    expect(describeGamepadSupport({}, false)).toBe("Gamepad API missing (secure context: no)");
  });

  it("says it fails when getGamepads throws", () => {
    const nav = {
      getGamepads: () => {
        throw new Error("SecurityError");
      },
    };
    expect(describeGamepadSupport(nav, false)).toBe(
      "Gamepad API fails: SecurityError (secure context: no)",
    );
  });
});
