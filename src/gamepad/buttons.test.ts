import { expect, test } from "vitest";
import { buttonName, layoutOf } from "./buttons";

test("Sony's vendor ID in the Gamepad API's id means a PlayStation layout", () => {
  // Chromium (WebView2) and WebKit/Firefox styles.
  expect(
    layoutOf("DUALSHOCK 4 Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 09cc)"),
  ).toBe("playstation");
  expect(layoutOf("054c-0ce6-DualSense Wireless Controller")).toBe("playstation");
  expect(layoutOf("Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)")).toBe(
    "xbox",
  );
  expect(layoutOf("Some pad (Vendor: 1054c Product: 0001)")).toBe("xbox");
});

test("buttons are named as printed on each controller", () => {
  expect(buttonName("xbox", "view")).toBe("View");
  expect(buttonName("playstation", "view")).toBe("Share");
  expect(buttonName("playstation", "lt")).toBe("L2");
});
