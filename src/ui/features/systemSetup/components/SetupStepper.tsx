import { cn } from "@/shared/lib/utils";
import { CheckIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

/** The steps of the guided setup, in order. */
export const SETUP_STEPS = ["system", "dataSource", "verify"] as const;

export type SetupStep = (typeof SETUP_STEPS)[number];

/** Shows where the user is in the guided setup. @param props - The step currently shown. @returns An ordered list that marks finished, current and upcoming steps. */
export function SetupStepper({ current }: { current: SetupStep }) {
  const { t } = useTranslation();
  const currentIndex = SETUP_STEPS.indexOf(current);
  return (
    <ol
      aria-label={t("setup.steps.label")}
      className="flex flex-wrap items-center gap-2 text-sm"
    >
      {SETUP_STEPS.map((step, index) => {
        const done = index < currentIndex;
        return (
          <li
            aria-current={index === currentIndex ? "step" : undefined}
            className={cn(
              "flex items-center gap-2 rounded-full border px-3 py-1.5",
              index === currentIndex &&
                "border-primary bg-primary/10 font-semibold text-primary",
              done && "border-success/40",
              index > currentIndex && "text-muted-foreground",
            )}
            key={step}
          >
            <span
              aria-hidden="true"
              className={cn(
                "flex size-5 items-center justify-center rounded-full bg-current/10 text-xs",
                // The colour marks the icon only; the label keeps text contrast.
                done && "text-success",
              )}
            >
              {done ? <CheckIcon className="size-3" /> : index + 1}
            </span>
            {t(`setup.steps.${step}`)}
          </li>
        );
      })}
    </ol>
  );
}
