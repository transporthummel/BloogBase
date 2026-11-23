import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import type { Job } from "../bindings";
import { JobRow } from "./JobList";

const job: Job = {
  id: 1,
  kind: "mod",
  subject: "aurora-bliss",
  phase: "waiting",
  received: 0,
  total: 0,
  error: null,
};

test("an update waiting for its game says so", () => {
  render(<JobRow job={job} label="Aurora" />);
  expect(screen.getByRole("status")).toHaveTextContent("Waiting for the game to end…");
});
