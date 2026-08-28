import { Button, Card, CardContent } from "@/shared/components";
import { CheckIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

/** System wizard success state properties. */
export interface SystemWizardSuccessProps {
  /** System name repeated back in the confirmation copy. */ name: string;
  /** Returns to the management overview. */ onBack: () => void;
  /** Whether an existing system was updated rather than created. */ updated: boolean;
}

/** Renders the terminal confirmation shown after a system was created or updated. @param props - System name, completion mode and the back callback. @returns The wizard success state. */
export function SystemWizardSuccess({
  name,
  onBack,
  updated,
}: SystemWizardSuccessProps) {
  const { t } = useTranslation();
  return (
    <Card className="mx-auto w-full max-w-3xl py-14 text-center shadow-lg">
      <CardContent className="flex flex-col items-center gap-4">
        <span className="flex size-12 items-center justify-center rounded-full bg-success/10 text-success">
          <CheckIcon />
        </span>
        <h1 className="text-xl font-extrabold">
          {t(
            updated
              ? "systemManagement.success.updated"
              : "systemManagement.success.created",
          )}
        </h1>
        <p className="text-sm text-muted-foreground">
          {t("systemManagement.success.description", { name })}
        </p>
        <Button onClick={onBack}>
          {t("systemManagement.actions.backToManagement")}
        </Button>
      </CardContent>
    </Card>
  );
}
