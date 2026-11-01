import { warn } from "@tauri-apps/plugin-log";
import { describe, expect, it, vi } from "vitest";
import { describeViolation, reportCspViolations } from "./cspReport";

vi.mock("@tauri-apps/plugin-log", () => ({ warn: vi.fn(() => Promise.resolve()) }));

function violation(directive: string, blockedURI: string) {
  return Object.assign(new Event("securitypolicyviolation"), {
    effectiveDirective: directive,
    blockedURI,
    sourceFile: "http://tauri.localhost/assets/index.js",
    lineNumber: 12,
  });
}

describe("describeViolation", () => {
  it("names the directive, the blocked source and where it came from", () => {
    expect(describeViolation(violation("img-src", "https://example.com/x.png"))).toBe(
      "CSP blocked img-src: https://example.com/x.png (http://tauri.localhost/assets/index.js:12)",
    );
  });

  it("calls inline code inline and cuts long URLs short", () => {
    expect(describeViolation({ ...violation("script-src-elem", ""), sourceFile: "" })).toBe(
      "CSP blocked script-src-elem: (inline)",
    );
    const line = describeViolation(
      violation("img-src", `data:image/png;base64,${"A".repeat(500)}`),
    );
    expect(line.length).toBeLessThan(200);
  });
});

describe("reportCspViolations", () => {
  it("logs a few violations per directive, then stops", () => {
    reportCspViolations();
    for (let i = 0; i < 8; i++) document.dispatchEvent(violation("img-src", `asset://x/${i}`));
    document.dispatchEvent(violation("connect-src", "https://example.com/"));
    expect(warn).toHaveBeenCalledTimes(6);
    expect(warn).toHaveBeenLastCalledWith(expect.stringContaining("connect-src"));
  });
});
