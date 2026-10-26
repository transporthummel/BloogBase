import { afterEach, describe, expect, it } from "vitest";
import { wantsContextMenu } from "./contextMenu";

describe("wantsContextMenu", () => {
  afterEach(() => {
    window.getSelection()?.removeAllRanges();
    document.body.innerHTML = "";
  });

  it("allows it in fields and editors", () => {
    document.body.innerHTML = `<input /><textarea></textarea><div contenteditable="true"><span>x</span></div>`;
    expect(wantsContextMenu(document.querySelector("input"))).toBe(true);
    expect(wantsContextMenu(document.querySelector("textarea"))).toBe(true);
    expect(wantsContextMenu(document.querySelector("span"))).toBe(true);
  });

  it("allows it over selected text only", () => {
    document.body.innerHTML = `<p>Keen</p><button>Play</button>`;
    const button = document.querySelector("button");
    expect(wantsContextMenu(button)).toBe(false);
    window.getSelection()?.selectAllChildren(document.querySelector("p")!);
    expect(wantsContextMenu(button)).toBe(true);
  });
});
