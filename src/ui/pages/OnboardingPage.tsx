import {
  createOnboarding,
  sendTestObservation,
  verifyTestObservation,
  type OnboardingInput,
  type OnboardingResult,
} from "@/features/onboarding";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
  Field,
  FieldGroup,
  FieldLabel,
  Input,
} from "@/shared/components";
import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";

const INITIAL: OnboardingInput = {
  instanceName: "",
  systemName: "",
  capacityWatts: 0,
  timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
  equipmentName: "",
  credentialName: "uploader",
};
/** Guides an administrator through first-system creation and ingestion verification. @returns The onboarding workflow. */
export function OnboardingPage() {
  const { t } = useTranslation();
  const [step, setStep] = useState(0);
  const [values, setValues] = useState(INITIAL);
  const [created, setCreated] = useState<OnboardingResult>();
  const [verified, setVerified] = useState(false);
  const create = useMutation({
    mutationFn: createOnboarding,
    onSuccess: (result) => {
      setCreated(result);
      setStep(3);
    },
  });
  const test = useMutation({
    mutationFn: async (result: OnboardingResult) => {
      await sendTestObservation(result);
      return verifyTestObservation(result.systemId);
    },
    onSuccess: (result) => {
      setVerified(result.accepted);
    },
  });
  const update = (field: keyof OnboardingInput, value: string) => {
    setValues((current) => ({
      ...current,
      [field]: field === "capacityWatts" ? Number(value) : value,
    }));
  };
  return (
    <main className="mx-auto flex min-h-screen max-w-2xl items-center px-6 py-10">
      <Card className="w-full">
        <CardHeader>
          <CardTitle aria-level={1} role="heading">
            {t("onboarding.title")}
          </CardTitle>
          <CardDescription>
            {t("onboarding.progress", { current: step + 1, total: 4 })}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {step === 0 ? (
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="instance-name">
                  {t("onboarding.instanceName")}
                </FieldLabel>
                <Input
                  id="instance-name"
                  onChange={(event) => {
                    update("instanceName", event.target.value);
                  }}
                  value={values.instanceName}
                />
              </Field>
            </FieldGroup>
          ) : null}
          {step === 1 ? (
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="system-name">
                  {t("onboarding.systemName")}
                </FieldLabel>
                <Input
                  id="system-name"
                  onChange={(event) => {
                    update("systemName", event.target.value);
                  }}
                  value={values.systemName}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="capacity">
                  {t("onboarding.capacity")}
                </FieldLabel>
                <Input
                  id="capacity"
                  min={1}
                  onChange={(event) => {
                    update("capacityWatts", event.target.value);
                  }}
                  type="number"
                  value={values.capacityWatts || ""}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="timezone">
                  {t("onboarding.timezone")}
                </FieldLabel>
                <Input
                  id="timezone"
                  onChange={(event) => {
                    update("timezone", event.target.value);
                  }}
                  value={values.timezone}
                />
              </Field>
            </FieldGroup>
          ) : null}
          {step === 2 ? (
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="equipment">
                  {t("onboarding.equipment")}
                </FieldLabel>
                <Input
                  id="equipment"
                  onChange={(event) => {
                    update("equipmentName", event.target.value);
                  }}
                  value={values.equipmentName}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="credential">
                  {t("onboarding.credential")}
                </FieldLabel>
                <Input
                  id="credential"
                  onChange={(event) => {
                    update("credentialName", event.target.value);
                  }}
                  value={values.credentialName}
                />
              </Field>
            </FieldGroup>
          ) : null}
          {step === 3 && created ? (
            <section aria-live="polite" className="flex flex-col gap-4">
              <h2 className="text-lg font-semibold">
                {t("onboarding.verifyTitle")}
              </h2>
              <p className="text-sm text-muted-foreground">
                {t("onboarding.secretGuidance")}
              </p>
              <code className="overflow-x-auto rounded-md bg-muted p-3 font-mono text-sm">
                {created.credentialSecret}
              </code>
              <p className="text-sm">
                {verified
                  ? t("onboarding.verified")
                  : t("onboarding.awaitingVerification")}
              </p>
            </section>
          ) : null}
        </CardContent>
        <CardFooter className="justify-between">
          <Button
            disabled={step === 0}
            onClick={() => {
              setStep((value) => Math.max(0, value - 1));
            }}
            variant="outline"
          >
            {t("onboarding.back")}
          </Button>
          {step < 2 ? (
            <Button
              onClick={() => {
                setStep((value) => value + 1);
              }}
            >
              {t("onboarding.next")}
            </Button>
          ) : null}
          {step === 2 ? (
            <Button
              disabled={create.isPending}
              onClick={() => {
                create.mutate(values);
              }}
            >
              {t("onboarding.create")}
            </Button>
          ) : null}
          {step === 3 && created ? (
            <Button
              disabled={test.isPending || verified}
              onClick={() => {
                test.mutate(created);
              }}
            >
              {t("onboarding.test")}
            </Button>
          ) : null}
        </CardFooter>
      </Card>
    </main>
  );
}
