/* eslint-disable max-lines-per-function, complexity -- one form over the system's general fields with shared validation and save state */
import {
  LocationAutocomplete,
  useUpdateSystem,
  type SystemRecord,
} from "@/entities/system";
import {
  generalDraft,
  generalIssues,
  updateSystemInput,
  type GeneralIssue,
} from "@/features/systemSettings/utils/generalDraft";
import { SessionRequestError } from "@/shared/api/sessionRequest";
import {
  Button,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
} from "@/shared/components";
import type { TFunction } from "i18next";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

function saveError(error: unknown, t: TFunction): string {
  if (error instanceof SessionRequestError) {
    if (error.status === 403)
      return t("systemSettings.general.errors.forbidden");
    if (error.status === 412)
      return t("systemSettings.general.errors.conflict");
    if (error.status === 422)
      return t(`setup.system.errors.fields.${error.detail ?? ""}`, {
        defaultValue: t("systemSettings.general.errors.invalid"),
      });
  }
  return t("systemSettings.general.errors.unavailable");
}

/** Edits a system's name, location, timezone, reporting interval, commissioning date and declared capacity. @param props - The stored system and whether it may only be viewed. @returns The general settings form. */
export function SystemGeneralForm({
  record,
  readOnly,
}: {
  record: SystemRecord;
  readOnly: boolean;
}) {
  const { t, i18n } = useTranslation();
  const [draft, setDraft] = useState(() => generalDraft(record));
  const [attempted, setAttempted] = useState(false);
  const update = useUpdateSystem(record.id);
  const issues = generalIssues(draft);
  const invalid = (field: GeneralIssue) => attempted && issues.includes(field);
  const patch = (next: Partial<typeof draft>) => {
    setDraft((current) => ({ ...current, ...next }));
    update.reset();
  };
  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    setAttempted(true);
    if (issues.length > 0) return;
    update.mutate(
      { version: record.version, input: updateSystemInput(draft, record) },
      {
        onSuccess: (saved) => {
          setDraft(generalDraft(saved));
          setAttempted(false);
        },
      },
    );
  }
  const derived =
    record.capacitySource === "derived" && record.derivedCapacityWatts !== null
      ? new Intl.NumberFormat(i18n.language, {
          maximumFractionDigits: 2,
        }).format(record.derivedCapacityWatts / 1_000)
      : null;
  return (
    <form className="flex flex-col gap-6" noValidate onSubmit={submit}>
      <fieldset className="contents" disabled={readOnly}>
        <FieldGroup>
          <Field data-invalid={invalid("name")}>
            <FieldLabel htmlFor="general-name">
              {t("setup.system.name")}
            </FieldLabel>
            <Input
              aria-invalid={invalid("name")}
              id="general-name"
              onChange={(event) => {
                patch({ name: event.target.value });
              }}
              value={draft.name}
            />
            {invalid("name") ? (
              <FieldError>{t("setup.system.nameRequired")}</FieldError>
            ) : null}
          </Field>
          <Field>
            <FieldLabel htmlFor="general-description">
              {t("setup.system.description")}
            </FieldLabel>
            <Input
              id="general-description"
              onChange={(event) => {
                patch({ description: event.target.value });
              }}
              value={draft.description}
            />
          </Field>
          <LocationAutocomplete
            onChange={(location) => {
              patch({ location });
            }}
            onSelect={(place) => {
              patch({ place });
            }}
            value={draft.location}
          />
          <Field data-invalid={invalid("timezone")}>
            <FieldLabel htmlFor="general-timezone">
              {t("setup.system.timezone")}
            </FieldLabel>
            <Input
              aria-invalid={invalid("timezone")}
              id="general-timezone"
              onChange={(event) => {
                patch({ timezone: event.target.value });
              }}
              value={draft.timezone}
            />
            {invalid("timezone") ? (
              <FieldError>
                {t("systemSettings.general.timezoneRequired")}
              </FieldError>
            ) : null}
          </Field>
          <Field data-invalid={invalid("statusInterval")}>
            <FieldLabel htmlFor="general-interval">
              {t("setup.system.statusInterval")}
            </FieldLabel>
            <Input
              aria-describedby="general-interval-hint"
              aria-invalid={invalid("statusInterval")}
              id="general-interval"
              inputMode="numeric"
              onChange={(event) => {
                patch({ statusIntervalSeconds: event.target.value });
              }}
              value={draft.statusIntervalSeconds}
            />
            <FieldDescription id="general-interval-hint">
              {t("setup.system.statusIntervalHint")}
            </FieldDescription>
            {invalid("statusInterval") ? (
              <FieldError>{t("setup.system.statusIntervalInvalid")}</FieldError>
            ) : null}
          </Field>
          <Field>
            <FieldLabel htmlFor="general-commissioning">
              {t("setup.system.commissioningDate")}
            </FieldLabel>
            <Input
              id="general-commissioning"
              onChange={(event) => {
                patch({ commissioningDate: event.target.value });
              }}
              type="date"
              value={draft.commissioningDate}
            />
          </Field>
          <Field data-invalid={invalid("capacity")}>
            <FieldLabel htmlFor="general-capacity">
              {t("setup.system.capacity")}
            </FieldLabel>
            <Input
              aria-describedby="general-capacity-hint"
              aria-invalid={invalid("capacity")}
              id="general-capacity"
              inputMode="decimal"
              onChange={(event) => {
                patch({ capacityKilowatts: event.target.value });
              }}
              value={draft.capacityKilowatts}
            />
            <FieldDescription id="general-capacity-hint">
              {derived
                ? t("systemSettings.general.capacityOverridden", {
                    capacity: derived,
                  })
                : t("setup.system.capacityHint")}
            </FieldDescription>
            {invalid("capacity") ? (
              <FieldError>{t("setup.system.capacityInvalid")}</FieldError>
            ) : null}
          </Field>
        </FieldGroup>
      </fieldset>
      {update.isError ? (
        <p className="text-sm text-destructive" role="alert">
          {saveError(update.error, t)}
        </p>
      ) : null}
      {update.isSuccess ? (
        <p className="text-sm text-success" role="status">
          {t("systemSettings.general.saved")}
        </p>
      ) : null}
      {readOnly ? null : (
        <Button
          className="self-start"
          disabled={update.isPending}
          type="submit"
        >
          {update.isPending
            ? t("systemSettings.general.saving")
            : t("systemSettings.general.save")}
        </Button>
      )}
    </form>
  );
}
