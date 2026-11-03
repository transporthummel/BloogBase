import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// The NSIS template is a copy of Tauri's (installer.nsi), so it has to be
// taken again whenever @tauri-apps/cli moves.
describe("NSIS template", () => {
  it("is the one of the Tauri CLI in use", () => {
    const pkg = JSON.parse(readFileSync("package.json", "utf8")) as {
      devDependencies: Record<string, string>;
    };
    const template = readFileSync("src-tauri/windows/installer.nsi", "utf8");
    const cli = pkg.devDependencies["@tauri-apps/cli"];
    expect(template).toContain(`from @tauri-apps/cli ${cli}:`);
  });
});
