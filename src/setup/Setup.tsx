import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { getSetupStep, setSetupStep } from "../preferences";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Notice } from "../ui/Notice";
import { setupStatusKey } from "./queries";
import {
  DoneStep,
  EngineStep,
  EpisodesStep,
  OwnedStep,
  StorageStep,
  WelcomeStep,
  type StepProps,
} from "./steps";

/** Wizard steps 1–5 and 7 of PLAN.md §7; "Try a mod" (6) arrives with the mod index. */
const STEPS = ["welcome", "storage", "engine", "episodes", "owned", "done"] as const;
type Step = (typeof STEPS)[number];

const components: Record<Step, (props: StepProps) => ReactNode> = {
  welcome: WelcomeStep,
  storage: StorageStep,
  engine: EngineStep,
  episodes: EpisodesStep,
  owned: OwnedStep,
  done: DoneStep,
};

function initialStep(): Step {
  const stored = getSetupStep();
  return STEPS.find((s) => s === stored) ?? "welcome";
}

/** First-run setup wizard. Resumes where it was left; every step can be skipped. */
export function Setup() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const client = useQueryClient();
  const [step, setStep] = useState(initialStep);
  const index = STEPS.indexOf(step);

  function go(to: Step) {
    setSetupStep(to);
    setStep(to);
    window.scrollTo?.(0, 0);
  }

  const finish = useMutation({
    mutationFn: () => unwrap(commands.completeSetup()),
    onSuccess: async () => {
      setSetupStep(null);
      await client.invalidateQueries({ queryKey: setupStatusKey });
      navigate({ to: "/" });
    },
  });

  const StepComponent = components[step];
  return (
    <div className="mx-auto grid max-w-2xl gap-6 px-4 py-8 sm:py-12">
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
