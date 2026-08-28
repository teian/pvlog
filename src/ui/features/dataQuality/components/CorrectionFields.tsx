import { Field, FieldGroup, FieldLabel, Input } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Editable values of a pending observation correction. */
export interface CorrectionDraft {
  /** Expected version, used for optimistic concurrency. */ expectedVersion: string;
  /** Replacement generation power; ignored in delete mode. */ generationPowerWatts: string;
  /** Observation the correction applies to. */ observationId: string;
  /** Audited reason for the change. */ reason: string;
}

/** Correction fields properties. */
export interface CorrectionFieldsProps {
  /** Whether the observation is deleted rather than corrected. */ deleteMode: boolean;
  /** Current draft values. */ draft: CorrectionDraft;
  /** Called with the changed subset of the draft. */ onChange: (
    patch: Partial<CorrectionDraft>,
  ) => void;
}

/** Renders the identifier, concurrency, reason, and value inputs of a correction request. @param props - Draft values, delete mode, and the change callback. @returns The correction input group. */
export function CorrectionFields({
  deleteMode,
  draft,
  onChange,
}: CorrectionFieldsProps) {
  const { t } = useTranslation();
  return (
    <FieldGroup>
      <Field>
        <FieldLabel htmlFor="observation-id">
          {t("dataQuality.correction.observationId")}
        </FieldLabel>
        <Input
          id="observation-id"
          onChange={(event) => {
            onChange({ observationId: event.target.value });
          }}
          required
          value={draft.observationId}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor="expected-version">
          {t("dataQuality.correction.expectedVersion")}
        </FieldLabel>
        <Input
          id="expected-version"
          min={1}
          onChange={(event) => {
            onChange({ expectedVersion: event.target.value });
          }}
          required
          type="number"
          value={draft.expectedVersion}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor="correction-reason">
          {t("dataQuality.correction.reason")}
        </FieldLabel>
        <Input
          id="correction-reason"
          onChange={(event) => {
            onChange({ reason: event.target.value });
          }}
          required
          value={draft.reason}
        />
      </Field>
      {deleteMode ? null : (
        <Field>
          <FieldLabel htmlFor="generation-power-watts">
            {t("dataQuality.correction.generationPowerWatts")}
          </FieldLabel>
          <Input
            id="generation-power-watts"
            onChange={(event) => {
              onChange({ generationPowerWatts: event.target.value });
            }}
            type="number"
            value={draft.generationPowerWatts}
          />
        </Field>
      )}
    </FieldGroup>
  );
}
