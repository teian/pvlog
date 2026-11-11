import type { DataSource } from "@/entities/dataSource";
import { useCreateSystem } from "@/entities/system";
import { SetupDataSourceStep } from "@/features/systemSetup/components/SetupDataSourceStep";
import {
  SetupStepper,
  type SetupStep,
} from "@/features/systemSetup/components/SetupStepper";
import { SetupSystemStep } from "@/features/systemSetup/components/SetupSystemStep";
import { SetupVerifyStep } from "@/features/systemSetup/components/SetupVerifyStep";
import {
  createSystemInput,
  emptySetupSystemDraft,
} from "@/features/systemSetup/utils/setupValues";
import { SessionRequestError } from "@/shared/api/sessionRequest";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Explains why a system could not be created in the user's language. */
function createError(error: unknown, t: TFunction): string | null {
  if (!error) return null;
  if (error instanceof SessionRequestError) {
    if (error.status === 403) return t("setup.system.errors.forbidden");
    if (error.status === 422)
      return t(`setup.system.errors.fields.${error.detail ?? ""}`, {
        defaultValue: t("setup.system.errors.invalid"),
      });
  }
  return t("setup.system.errors.unavailable");
}

/**
 * Guides from nothing to a system that verifiably receives data.
 *
 * The system exists as soon as the first step is submitted, so the later
 * steps can be skipped and resumed from the system's settings.
 *
 * @param props - An existing system to resume with, and the callback that receives the system once the setup ends.
 * @returns The three-step guided setup.
 */
export function SystemSetupWizard({
  resumeSystemId,
  onFinished,
}: {
  resumeSystemId?: string;
  onFinished: (systemId: string) => void;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(emptySetupSystemDraft);
  const [systemId, setSystemId] = useState(resumeSystemId);
  const [step, setStep] = useState<SetupStep>(
    resumeSystemId ? "dataSource" : "system",
  );
  const [dataSource, setDataSource] = useState<DataSource>();
  const create = useCreateSystem();

  return (
    <Card className="mx-auto w-full max-w-3xl">
      <CardHeader className="gap-4">
        <div className="flex flex-col gap-1">
          <CardTitle>
            <h1 className="text-xl font-extrabold">{t("setup.title")}</h1>
          </CardTitle>
          <CardDescription>{t(`setup.descriptions.${step}`)}</CardDescription>
        </div>
        <SetupStepper current={step} />
      </CardHeader>
      <CardContent>
        {step === "system" ? (
          <SetupSystemStep
            draft={draft}
            errorMessage={createError(create.error, t)}
            onChange={(next) => {
              setDraft((current) => ({ ...current, ...next }));
            }}
            onSubmit={() => {
              create.mutate(
                createSystemInput(
                  draft,
                  Intl.DateTimeFormat().resolvedOptions().timeZone,
                ),
                {
                  onSuccess: (record) => {
                    setSystemId(record.id);
                    setStep("dataSource");
                  },
                },
              );
            }}
            pending={create.isPending}
          />
        ) : null}
        {step === "dataSource" && systemId ? (
          <SetupDataSourceStep
            onConnected={(source) => {
              setDataSource(source);
              setStep("verify");
            }}
            onSkip={() => {
              onFinished(systemId);
            }}
            systemId={systemId}
          />
        ) : null}
        {step === "verify" && systemId ? (
          <SetupVerifyStep
            dataSource={dataSource}
            onFinish={() => {
              onFinished(systemId);
            }}
            systemId={systemId}
          />
        ) : null}
      </CardContent>
    </Card>
  );
}
