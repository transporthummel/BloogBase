import { QueryClient } from "@tanstack/react-query";
import { expect, test, vi } from "vitest";
import { commands, type Profile } from "../bindings";
import { coverKey, forgetRemovedCovers } from "./covers";

vi.mock("../bindings", () => ({ commands: { listProfiles: vi.fn() } }));

test("forgets covers of removed profiles and of copies a mod is no longer on", async () => {
  const client = new QueryClient();
  const cover = { src: "x.png", scrambled: false };
  client.setQueryData([...coverKey("keen5-1"), "keen5-1"], cover);
  client.setQueryData([...coverKey("keen4-1"), "keen4-1"], cover);
  // The game it was on is gone; a copy added later gets the same id.
  client.setQueryData([...coverKey("level-pack"), "keen5-1"], cover);
  vi.mocked(commands.listProfiles).mockResolvedValue({
    status: "ok",
    data: [
      { id: "keen4-1", installId: "keen4-1" },
      { id: "level-pack", installId: null },
    ] as Profile[],
  });

  await forgetRemovedCovers(client);
  expect(client.getQueryData([...coverKey("keen4-1"), "keen4-1"])).toEqual(cover);
  expect(client.getQueryData([...coverKey("keen5-1"), "keen5-1"])).toBeUndefined();
  expect(client.getQueryData([...coverKey("level-pack"), "keen5-1"])).toBeUndefined();
});
