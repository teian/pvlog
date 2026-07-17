/* eslint-disable max-lines, max-lines-per-function -- the handoff specifies a single-page wizard whose two numbered sections share one atomic draft */
import { InverterDraftEditor } from "@/features/systemManagement/components/InverterDraftEditor";
import { LocationAutocomplete } from "@/features/systemManagement/components/LocationAutocomplete";
import { SystemWizardSectionHeading } from "@/features/systemManagement/components/SystemWizardSectionHeading";
import type {
  ManagedSystem,
  SystemWizardDraft,
} from "@/features/systemManagement/types/systemManagement.types";
import {
  emptyInverter,
  systemDraft,
} from "@/features/systemManagement/utils/systemManagementDraft";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Card,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
  Field,
  FieldGroup,
  FieldLabel,
  Input,
  ToggleGroup,
  ToggleGroupItem,
} from "@/shared/components";
import { SessionRequestError } from "@/shared/api/sessionRequest";
import {
  ArrowLeftIcon,
  CheckIcon,
  MinusIcon,
  PlusIcon,
  Table2Icon,
} from "lucide-react";
import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Returns the first user-actionable reason why a system draft cannot be saved. */
function validationIssue(
  draft: SystemWizardDraft,
  t: TFunction,
): string | null {
  if (!draft.name.trim())
    return t("systemManagement.wizard.validation.systemName");
  for (const [inverterIndex, inverter] of draft.inverters.entries()) {
    const inverterNumber = inverterIndex + 1;
    if (!inverter.name.trim())
      return t("systemManagement.wizard.validation.inverterName", {
        number: inverterNumber,
      });
    if (inverter.strings.length === 0)
      return t("systemManagement.wizard.validation.strings", {
        number: inverterNumber,
      });
    for (const [stringIndex, string] of inverter.strings.entries()) {
      const context = {
        inverter: inverterNumber,
        string: stringIndex + 1,
      };
      if (!string.name.trim())
        return t("systemManagement.wizard.validation.stringName", context);
      if (string.panelCount <= 0)
        return t("systemManagement.wizard.validation.moduleCount", context);
      if (string.modulePeakPowerWatts <= 0)
        return t("systemManagement.wizard.validation.modulePower", context);
    }
  }
  return null;
}

/** Maps a save failure to a localized, actionable explanation. */
function saveErrorDescription(error: unknown, t: TFunction): string {
  if (!(error instanceof SessionRequestError))
    return t("systemManagement.errors.saveDescription");
  if (error.status === 403)
    return t("systemManagement.errors.saveForbiddenDescription");
  if (error.status === 409 || error.status === 412)
    return t("systemManagement.errors.saveConflictDescription");
  if (error.status === 422)
    return t("systemManagement.errors.saveValidationDescription");
  if (error.status === 503)
    return t("systemManagement.errors.saveUnavailableDescription");
  return t("systemManagement.errors.saveDescription");
}

/** Renders the single-page create/edit system wizard from the design handoff. @param props - Existing system, mutation state and completion/cancel callbacks. @returns Sectioned accessible wizard or success state. */
export function SystemWizard({
  current,
  pending,
  error,
  onCancel,
  onSubmit,
}: {
  current?: ManagedSystem;
  pending: boolean;
  error: unknown;
  onCancel: () => void;
  onSubmit: (draft: SystemWizardDraft) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(() => systemDraft(current));
  const [done, setDone] = useState(false);
  const patch = (next: Partial<SystemWizardDraft>) => {
    setDraft((value) => ({ ...value, ...next }));
  };
  if (done)
    return (
      <Card className="mx-auto w-full max-w-3xl py-14 text-center shadow-lg">
        <CardContent className="flex flex-col items-center gap-4">
          <span className="flex size-12 items-center justify-center rounded-full bg-success/10 text-success">
            <CheckIcon />
          </span>
          <h1 className="text-xl font-extrabold">
            {t(
              current
                ? "systemManagement.success.updated"
                : "systemManagement.success.created",
            )}
          </h1>
          <p className="text-sm text-muted-foreground">
            {t("systemManagement.success.description", { name: draft.name })}
          </p>
          <Button onClick={onCancel}>
            {t("systemManagement.actions.backToManagement")}
          </Button>
        </CardContent>
      </Card>
    );
  const issue = validationIssue(draft, t);
  const valid = issue === null;
  return (
    <div className="flex flex-col gap-4">
      <Button className="self-start" onClick={onCancel} variant="link">
        <ArrowLeftIcon data-icon="inline-start" />
        {t("systemManagement.actions.backToManagement")}
      </Button>
      <Card className="mx-auto w-full max-w-3xl gap-0 overflow-hidden py-0 shadow-lg">
        <CardHeader className="border-b py-5">
          <div className="flex items-center gap-3">
            <span className="flex size-9 items-center justify-center rounded-md bg-primary text-primary-foreground">
              <Table2Icon />
            </span>
            <div>
              <CardTitle>
                <h2 className="text-base font-extrabold">
                  {t(
                    current
                      ? "systemManagement.wizard.editTitle"
                      : "systemManagement.wizard.createTitle",
                  )}
                </h2>
              </CardTitle>
              <p className="mt-1 text-xs text-muted-foreground">
                {t("systemManagement.wizard.subtitle")}
              </p>
            </div>
          </div>
        </CardHeader>
        <CardContent className="flex flex-col gap-8 py-6">
          <section className="flex flex-col gap-4">
            <SystemWizardSectionHeading number={1}>
              {t("systemManagement.wizard.systemData")}
            </SystemWizardSectionHeading>
            <FieldGroup className="gap-4">
              <Field data-invalid={!draft.name.trim()}>
                <FieldLabel htmlFor="system-name">
                  {t("systemManagement.wizard.systemName")}
                </FieldLabel>
                <Input
                  aria-invalid={!draft.name.trim()}
                  id="system-name"
                  onChange={(event) => {
                    patch({ name: event.target.value });
                  }}
                  value={draft.name}
                />
              </Field>
              <LocationAutocomplete
                onChange={(location) => {
                  patch({ location });
                }}
                value={draft.location}
              />
              <div className="grid gap-5 sm:grid-cols-2">
                <Field>
                  <FieldLabel>{t("systemManagement.wizard.status")}</FieldLabel>
                  <ToggleGroup
                    onValueChange={(value) => {
                      if (value) patch({ active: value === "active" });
                    }}
                    type="single"
                    value={draft.active ? "active" : "inactive"}
                    variant="outline"
                  >
                    <ToggleGroupItem value="active">
                      {t("systemManagement.status.active")}
                    </ToggleGroupItem>
                    <ToggleGroupItem value="inactive">
                      {t("systemManagement.status.inactive")}
                    </ToggleGroupItem>
                  </ToggleGroup>
                </Field>
                <Field>
                  <FieldLabel>
                    {t("systemManagement.wizard.inverterCount")}
                  </FieldLabel>
                  <div className="flex w-fit items-center rounded-md border">
                    <Button
                      aria-label={t(
                        "systemManagement.actions.decreaseInverters",
                      )}
                      disabled={draft.inverters.length <= 1}
                      onClick={() => {
                        patch({ inverters: draft.inverters.slice(0, -1) });
                      }}
                      size="icon-sm"
                      type="button"
                      variant="ghost"
                    >
                      <MinusIcon />
                    </Button>
                    <span className="w-10 text-center font-mono text-sm">
                      {draft.inverters.length}
                    </span>
                    <Button
                      aria-label={t(
                        "systemManagement.actions.increaseInverters",
                      )}
                      disabled={draft.inverters.length >= 12}
                      onClick={() => {
                        patch({
                          inverters: [
                            ...draft.inverters,
                            emptyInverter(draft.inverters.length + 1),
                          ],
                        });
                      }}
                      size="icon-sm"
                      type="button"
                      variant="ghost"
                    >
                      <PlusIcon />
                    </Button>
                  </div>
                </Field>
              </div>
            </FieldGroup>
          </section>
          <section className="flex flex-col gap-4">
            <SystemWizardSectionHeading number={2}>
              {t("systemManagement.wizard.inverters")}
            </SystemWizardSectionHeading>
            <p className="text-xs text-muted-foreground">
              {t("systemManagement.wizard.inverterDescription")}
            </p>
            {draft.inverters.map((inverter, index) => (
              <InverterDraftEditor
                canRemove={draft.inverters.length > 1}
                index={index}
                key={inverter.id ?? `new-${String(index)}`}
                onChange={(next) => {
                  patch({
                    inverters: draft.inverters.map(
                      (currentValue, currentIndex) =>
                        currentIndex === index ? next : currentValue,
                    ),
                  });
                }}
                onRemove={() => {
                  patch({
                    inverters: draft.inverters.filter(
                      (_, currentIndex) => currentIndex !== index,
                    ),
                  });
                }}
                value={inverter}
              />
            ))}
            <Button
              className="self-start border-dashed"
              onClick={() => {
                patch({
                  inverters: [
                    ...draft.inverters,
                    emptyInverter(draft.inverters.length + 1),
                  ],
                });
              }}
              type="button"
              variant="outline"
            >
              <PlusIcon data-icon="inline-start" />
              {t("systemManagement.wizard.addInverter")}
            </Button>
          </section>
          {error ? (
            <Alert variant="destructive">
              <AlertTitle>{t("systemManagement.errors.saveTitle")}</AlertTitle>
              <AlertDescription>
                {saveErrorDescription(error, t)}
              </AlertDescription>
            </Alert>
          ) : null}
        </CardContent>
        <CardFooter className="justify-between gap-3 border-t bg-card py-4">
          <Button onClick={onCancel} type="button" variant="outline">
            {t("systemManagement.actions.cancel")}
          </Button>
          <div className="flex items-center gap-3">
            <span
              aria-live="polite"
              className={
                issue
                  ? "text-right text-xs text-destructive"
                  : "hidden text-xs text-muted-foreground sm:inline"
              }
              id="system-wizard-save-status"
            >
              {issue
                ? t("systemManagement.wizard.validation.blocked", {
                    reason: issue,
                  })
                : t("systemManagement.wizard.footerHint")}
            </span>
            <Button
              aria-describedby="system-wizard-save-status"
              disabled={!valid || pending}
              onClick={() => {
                void onSubmit(draft)
                  .then(() => {
                    setDone(true);
                  })
                  .catch(() => undefined);
              }}
              type="button"
            >
              {pending
                ? t("systemManagement.actions.saving")
                : t(
                    current
                      ? "systemManagement.actions.updateSystem"
                      : "systemManagement.actions.createSystem",
                  )}
            </Button>
          </div>
        </CardFooter>
      </Card>
    </div>
  );
}
