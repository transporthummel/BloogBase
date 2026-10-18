import { expect, test } from "vitest";
import { linkify } from "./readme";

test("links are http(s) URLs without trailing punctuation", () => {
  expect(linkify("Visit https://pckf.com/x?a=1. Or (http://keenwiki.shikadi.net)!")).toEqual([
    { text: "Visit " },
    { url: "https://pckf.com/x?a=1" },
    { text: ". Or (" },
    { url: "http://keenwiki.shikadi.net" },
    { text: ")!" },
  ]);
});

test("other schemes stay text", () => {
  expect(linkify("javascript:alert(1) file:///C:/x ftp://a.b")).toEqual([
    { text: "javascript:alert(1) file:///C:/x ftp://a.b" },
  ]);
  expect(linkify("")).toEqual([]);
});
