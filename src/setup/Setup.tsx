import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands, type SetupStatus } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Notice } from "../ui/Notice";
import { setupStatusKey, useSetupStatus } from "./queries";
import {
  DoneStep,
  EngineStep,
  EpisodesStep,
  ModsStep,
  OwnedStep,
  StorageStep,
  ThemeStep,
  WelcomeStep,
  type StepProps,
} from "./steps";

/** The wizard steps of PLAN.md §7. */
const STEPS = [
  "welcome",
  "storage",
  "engine",
  "owned",
  "episodes",
  "mods",
  "theme",
  "done",
] as const;
type Step = (typeof STEPS)[number];

const components: Record<Step, (props: StepProps) => ReactNode> = {
  welcome: WelcomeStep,
  storage: StorageStep,
  engine: EngineStep,
  episodes: EpisodesStep,
  owned: OwnedStep,
  mods: ModsStep,
  theme: ThemeStep,
  done: DoneStep,
};

/** First-run setup wizard. Resumes where it was left; every step can be skipped. */
export function Setup() {
  const status = useSetupStatus();
  // The library keeps the step to resume at.
  if (!status.data) return null;
  return <Wizard resume={status.data.step} />;
}

function Wizard({ resume }: { resume: string | null }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const client = useQueryClient();
  const [step, setStep] = useState<Step>(() => STEPS.find((s) => s === resume) ?? "welcome");
  const index = STEPS.indexOf(step);

  function go(to: Step) {
    // Not fatal: the wizard just starts over after a restart.
    commands.setSetupStep(to).catch(() => {});
    client.setQueryData<SetupStatus>(setupStatusKey, (s) => s && { ...s, step: to });
    setStep(to);
    window.scrollTo?.(0, 0);
  }

  const finish = useMutation({
    mutationFn: () => unwrap(commands.completeSetup()),
    onSuccess: async () => {
      // The status query has no observer on most steps; refetch it anyway so
      // the library doesn't see the stale "not completed" and send us back.
      await client.invalidateQueries({ queryKey: setupStatusKey, refetchType: "all" });
      navigate({ to: "/" });
    },
  });

  const StepComponent = components[step];
  return (
    <div className="mx-auto grid max-w-2xl gap-6 px-4 py-8">
      <header className="flex min-h-10 items-center gap-3">
        <span className="font-pixel text-sm text-accent">BloogBase</span>
        {step !== "done" && (
          <Button
            variant="ghost"
            className="ml-auto"
            onClick={() => finish.mutate()}
            disabled={finish.isPending}
          >
            {t("setup.skipAll")}
          </Button>
        )}
      </header>
      <ol aria-label={t("setup.progress")} className="flex gap-2">
        {STEPS.map((s, i) => (
          <li
            key={s}
            aria-current={i === index ? "step" : undefined}
            className={`h-1.5 flex-1 rounded-full transition-colors ${
              i <= index ? "bg-accent" : "bg-surface-raised"
            }`}
          >
            <span className="sr-only">{t(`setup.${s}.title`)}</span>
          </li>
        ))}
      </ol>
      <Card className="grid gap-5 p-6 sm:p-8">
        <StepComponent
          next={() => go(STEPS[Math.min(index + 1, STEPS.length - 1)])}
          back={index > 0 ? () => go(STEPS[index - 1]) : undefined}
          finish={() => finish.mutate()}
          finishing={finish.isPending}
        />
        {finish.error && <Notice tone="error">{errorMessage(t, finish.error)}</Notice>}
      </Card>
    </div>
  );
}
