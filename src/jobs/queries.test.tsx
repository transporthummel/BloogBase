import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { events, type IndexMod, type Job } from "../bindings";
import { indexModsKey } from "../discover/queries";
import { useJobEvents } from "./queries";

vi.mock("../bindings", () => ({
  commands: {},
  events: { jobUpdated: { listen: vi.fn() } },
}));

const done: Job = {
  id: 1,
  kind: "mod",
  subject: "aurora-bliss",
  phase: "done",
  received: 0,
  total: 0,
  error: null,
};

function Events() {
  useJobEvents();
  return null;
}

/** Renders the listener and returns how to send it a job update. */
function listen(client: QueryClient) {
  let emit: (job: Job) => void = () => {};
  vi.mocked(events.jobUpdated.listen).mockImplementation((handler) => {
    emit = (job) => handler({ payload: job } as never);
    return Promise.resolve(() => {});
  });
  render(
    <QueryClientProvider client={client}>
      <Events />
    </QueryClientProvider>,
  );
  return (job: Job) => emit(job);
}

test("an updated index mod gets its cover and README read again", () => {
  const client = new QueryClient();
  const invalidate = vi.spyOn(client, "invalidateQueries");
  const mods = [
    { entry: { id: "aurora-bliss" }, modId: "aurora", updateAvailable: true },
    { entry: { id: "fresh" }, modId: null, updateAvailable: false },
  ] as IndexMod[];
  client.setQueryData(indexModsKey, mods);
  const emit = listen(client);

  emit(done);
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["cover", "aurora"] });
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["readme", "aurora"] });

  // A first install has no cover to refresh.
  invalidate.mockClear();
  emit({ ...done, subject: "fresh" });
  expect(invalidate).not.toHaveBeenCalledWith(
    expect.objectContaining({ queryKey: expect.arrayContaining(["cover"]) }),
  );
});

test("without the index at hand, all covers and READMEs are refreshed", () => {
  const client = new QueryClient();
  const invalidate = vi.spyOn(client, "invalidateQueries");
  listen(client)(done);
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["cover"] });
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["readme"] });
});
