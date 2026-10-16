import type { CommandError } from "./bindings";

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: CommandError };

/** Unwraps a typed command result so it can be used with TanStack Query. */
export async function unwrap<T>(result: Promise<Result<T>>): Promise<T> {
  const res = await result;
  if (res.status === "error") throw res.error;
  return res.data;
}

export function isCommandError(err: unknown): err is CommandError {
  return typeof err === "object" && err !== null && "kind" in err && "message" in err;
}
