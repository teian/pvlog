/* eslint-disable max-lines-per-function -- one form whose optional fields share a single draft and submit */
import { LocationAutocomplete } from "@/entities/system";
import {
  setupSystemIssues,
  type SetupSystemDraft,
} from "@/features/systemSetup/utils/setupValues";
import {
  Button,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
} from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** First setup step: the system itself, where only the name is required. @param props - Draft, change and submit callbacks, and the state of the creation request. @returns A form whose rarely needed fields sit behind one disclosure. */
export function SetupSystemStep({
  draft,
  pending,
  errorMessage,
  onChange,
  onSubmit,
}: {
  draft: SetupSystemDraft;
  pending: boolean;
  errorMessage: string | null;
  onChange: (next: Partial<SetupSystemDraft>) => void;
  onSubmit: () => void;
}) {
  const { t } = useTranslation();
  const [attempted, setAttempted] = useState(false);
  const issues = setupSystemIssues(draft);
  const invalid = (field: (typeof issues)[number]) =>
    attempted && issues.includes(field);
  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    setAttempted(true);
    if (issues.length === 0) onSubmit();
  }
  return (
    <form className="flex flex-col gap-6" noValidate onSubmit={submit}>
      <FieldGroup>
        <Field data-invalid={invalid("name")}>
          <FieldLabel htmlFor="setup-name">{t("setup.system.name")}</FieldLabel>
          <Input
            aria-describedby={invalid("name") ? "setup-name-error" : undefined}
            aria-invalid={invalid("name")}
            autoComplete="off"
            id="setup-name"
            onChange={(event) => {
              onChange({ name: event.target.value });
            }}
            required
            value={draft.name}
          />
          {invalid("name") ? (
            <FieldError id="setup-name-error">
              {t("setup.system.nameRequired")}
            </FieldError>
          ) : null}
        </Field>
        <LocationAutocomplete
          onChange={(location) => {
            onChange({ location });
          }}
          onSelect={(place) => {
            onChange({ place });
          }}
          value={draft.location}
        />
        <Field data-invalid={invalid("capacity")}>
          <FieldLabel htmlFor="setup-capacity">
            {t("setup.system.capacity")}
          </FieldLabel>
          <Input
            aria-describedby="setup-capacity-hint"
            aria-invalid={invalid("capacity")}
            id="setup-capacity"
            inputMode="decimal"
            onChange={(event) => {
              onChange({ capacityKilowatts: event.target.value });
            }}
            value={draft.capacityKilowatts}
          />
          <FieldDescription id="setup-capacity-hint">
            {t("setup.system.capacityHint")}
          </FieldDescription>
          {invalid("capacity") ? (
            <FieldError>{t("setup.system.capacityInvalid")}</FieldError>
          ) : null}
        </Field>
      </FieldGroup>
      <details className="rounded-md border p-4">
        <summary className="cursor-pointer text-sm font-semibold">
          {t("setup.system.moreOptions")}
        </summary>
        <FieldGroup className="mt-4">
          <Field>
            <FieldLabel htmlFor="setup-timezone">
              {t("setup.system.timezone")}
            </FieldLabel>
            <Input
              aria-describedby="setup-timezone-hint"
              id="setup-timezone"
              onChange={(event) => {
                onChange({ timezone: event.target.value });
              }}
              placeholder={t("setup.system.timezonePlaceholder")}
              value={draft.timezone}
            />
            <FieldDescription id="setup-timezone-hint">
              {t("setup.system.timezoneHint")}
            </FieldDescription>
          </Field>
          <Field data-invalid={invalid("statusInterval")}>
            <FieldLabel htmlFor="setup-interval">
              {t("setup.system.statusInterval")}
            </FieldLabel>
            <Input
              aria-describedby="setup-interval-hint"
              aria-invalid={invalid("statusInterval")}
              id="setup-interval"
              inputMode="numeric"
              onChange={(event) => {
                onChange({ statusIntervalSeconds: event.target.value });
              }}
              placeholder="300"
              value={draft.statusIntervalSeconds}
            />
            <FieldDescription id="setup-interval-hint">
              {t("setup.system.statusIntervalHint")}
            </FieldDescription>
            {invalid("statusInterval") ? (
              <FieldError>{t("setup.system.statusIntervalInvalid")}</FieldError>
            ) : null}
          </Field>
          <Field>
            <FieldLabel htmlFor="setup-commissioning">
              {t("setup.system.commissioningDate")}
            </FieldLabel>
            <Input
              id="setup-commissioning"
              onChange={(event) => {
                onChange({ commissioningDate: event.target.value });
              }}
              type="date"
              value={draft.commissioningDate}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="setup-description">
              {t("setup.system.description")}
            </FieldLabel>
            <Input
              id="setup-description"
              onChange={(event) => {
                onChange({ description: event.target.value });
              }}
              value={draft.description}
            />
          </Field>
        </FieldGroup>
      </details>
      {errorMessage ? (
        <p className="text-sm text-destructive" role="alert">
          {errorMessage}
        </p>
      ) : null}
      <Button className="self-start" disabled={pending} type="submit">
        {pending ? t("setup.system.creating") : t("setup.system.create")}
      </Button>
    </form>
  );
}
