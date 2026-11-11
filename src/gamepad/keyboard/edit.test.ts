import { describe, expect, it, vi } from "vitest";
import { backspace, insert, moveCaret, readField, writeField, type Text } from "./edit";

const text = (value: string, start = value.length, end = start, maxLength = -1): Text => ({
  value,
  start,
  end,
  maxLength,
});

describe("insert", () => {
  it("types at the caret", () => {
    expect(insert(text("Kn", 1), "e")).toEqual(text("Ken", 2));
  });

  it("replaces a selection", () => {
    expect(insert(text("Keen 4", 0, 4), "Bloog")).toEqual(text("Bloog 4", 5));
  });

  it("stops at maxLength", () => {
    expect(insert(text("abc", 3, 3, 4), "de")).toEqual(text("abcd", 4, 4, 4));
    expect(insert(text("abcd", 4, 4, 4), "e")).toEqual(text("abcd", 4, 4, 4));
    expect(insert(text("abcd", 0, 2, 4), "xyz")).toEqual(text("xycd", 2, 2, 4));
  });
});

describe("backspace", () => {
  it("deletes the character before the caret", () => {
    expect(backspace(text("Keen", 2))).toEqual(text("Ken", 1));
    expect(backspace(text("Keen", 0))).toEqual(text("Keen", 0));
  });

  it("deletes a selection", () => {
    expect(backspace(text("Keen 4", 4, 6))).toEqual(text("Keen", 4));
  });

  it("deletes a whole character outside the basic plane", () => {
    expect(backspace(text("a😀"))).toEqual(text("a"));
  });
});

describe("moveCaret", () => {
  it("moves one character, within the text", () => {
    expect(moveCaret(text("ab", 1), -1)).toEqual(text("ab", 0));
    expect(moveCaret(text("ab", 0), -1)).toEqual(text("ab", 0));
    expect(moveCaret(text("ab", 2), 1)).toEqual(text("ab", 2));
  });

  it("collapses a selection to its side", () => {
    expect(moveCaret(text("abcd", 1, 3), -1)).toEqual(text("abcd", 1));
    expect(moveCaret(text("abcd", 1, 3), 1)).toEqual(text("abcd", 3));
  });
});

describe("fields", () => {
  it("writes so an input event follows and puts the caret back", () => {
    const field = document.createElement("input");
    field.value = "Ken";
    const onInput = vi.fn();
    field.addEventListener("input", onInput);
    writeField(field, text("Keen", 2));
    expect(field.value).toBe("Keen");
    expect(onInput).toHaveBeenCalledOnce();
    expect([field.selectionStart, field.selectionEnd]).toEqual([2, 2]);
  });

  it("types at the end of a number field, which has no caret", () => {
    const field = document.createElement("input");
    field.type = "number";
    field.value = "1";
    expect(readField(field)).toEqual(text("1"));
    writeField(field, insert(readField(field), "2"));
    expect(field.value).toBe("12");
  });
});
