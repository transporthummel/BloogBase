import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { expect, test, vi } from "vitest";
import { commands, type Profile } from "../bindings";
import { coverKey, forgetRemovedCovers, useCover } from "./covers";

vi.mock("../bindings", () => ({ commands: { listProfiles: vi.fn(), profileCover: vi.fn() } }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}` }));

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

test("a cover that fails to show is asked for once more, then left to the placeholder", async () => {
  const client = new QueryClient();
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client }, children);
  vi.mocked(commands.profileCover).mockResolvedValue({
    status: "ok",
    data: { path: "/covers/keen4-1.png", scrambled: false },
  });
  // Each extraction gets its own URL.
  let now = 1;
  vi.spyOn(Date, "now").mockImplementation(() => now++);
  const { result } = renderHook(() => useCover("keen4-1", "keen4-1"), { wrapper });
  await waitFor(() => expect(result.current.data).toBeTruthy());
  const first = result.current.data!.src;

  // An image already replaced changes nothing.
  act(() => result.current.onError("asset://elsewhere.png"));
  expect(commands.profileCover).toHaveBeenCalledTimes(1);

  act(() => result.current.onError(first));
  await waitFor(() => expect(commands.profileCover).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(result.current.data?.src).not.toBe(first));

  act(() => result.current.onError(result.current.data!.src));
  await waitFor(() => expect(result.current.data).toBeNull());
  expect(commands.profileCover).toHaveBeenCalledTimes(2);
});
