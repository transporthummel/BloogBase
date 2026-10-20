import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { commands, events, type Job, type JobKind } from "../bindings";
import { freeGamesKey, indexModsKey } from "../discover/queries";
import { installsKey, profilesKey } from "../library/queries";
import { runtimeKey } from "../runtime/queries";

export const jobsKey = ["jobs"] as const;

export function useJobs() {
  return useQuery({ queryKey: jobsKey, queryFn: commands.listJobs });
}

/** The latest job doing this work, if any. */
export function useJob(kind: JobKind, subject: string | null = null) {
  return useJobs().data?.find((j) => j.kind === kind && j.subject === subject);
}

export function isFinished(job: Job) {
  return job.phase === "done" || job.phase === "failed";
}

function upsert(client: QueryClient, job: Job) {
  client.setQueryData<Job[]>(jobsKey, (jobs = []) => {
    // A retry replaces the earlier attempt at the same work.
    const others = jobs.filter(
      (j) => j.id !== job.id && !(j.kind === job.kind && j.subject === job.subject),
    );
    return [...others, job];
  });
}

/** Keeps the jobs cache current and refreshes what a finished job changed. */
export function useJobEvents() {
  const client = useQueryClient();
  useEffect(() => {
    const unlisten = events.jobUpdated.listen(({ payload: job }) => {
      upsert(client, job);
      if (job.phase !== "done") return;
      if (job.kind === "runtime") client.invalidateQueries({ queryKey: runtimeKey });
      if (job.kind === "freeGame") {
        client.invalidateQueries({ queryKey: profilesKey });
        client.invalidateQueries({ queryKey: installsKey });
        client.invalidateQueries({ queryKey: freeGamesKey });
      }
      if (job.kind === "mod") {
        client.invalidateQueries({ queryKey: profilesKey });
        client.invalidateQueries({ queryKey: indexModsKey });
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [client]);
}

/** Starts a job through a command that returns it right away. */
export function useStartJob<A>(start: (arg: A) => Promise<Job>) {
  const client = useQueryClient();
  return useMutation({ mutationFn: start, onSuccess: (job) => upsert(client, job) });
}
