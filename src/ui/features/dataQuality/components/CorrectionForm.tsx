import { useCorrectObservation } from "@/features/dataQuality/hooks/useCorrectObservation";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Field,
  FieldGroup,
  FieldLabel,
  Input,
  Toggle,
} from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Correction form properties. */
export interface CorrectionFormProps {
  /** System that owns the observation being corrected. */ systemId: string;
  /** Called after a correction/deletion is accepted, to start the reconciliation-progress indicator. */
  onSubmitted: () => void;
}

/** Lets an authorized user correct or delete one observation by ID, with optimistic concurrency and audited reasons. @param props - Target system and a post-submit callback. @returns The correction form card. */
export function CorrectionForm({ systemId, onSubmitted }: CorrectionFormProps) {
  const { t } = useTranslation();
  const mutation = useCorrectObservation();
  const [observationId, setObservationId] = useState("");
  const [expectedVersion, setExpectedVersion] = useState("");
  const [reason, setReason] = useState("");
  const [generationPowerWatts, setGenerationPowerWatts] = useState("");
  const [deleteMode, setDeleteMode] = useState(false);

  const submitCorrection = () => {
    mutation.mutate(
      {
        systemId,
        observationId,
        expectedVersion: Number(expectedVersion),
        reason,
        delete: deleteMode,
        ...(deleteMode || generationPowerWatts === ""
          ? {}
          : { generationPowerWatts: Number(generationPowerWatts) }),
      },
      { onSuccess: onSubmitted },
    );
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("dataQuality.correction.title")}</CardTitle>
        <CardDescription>
          {t("dataQuality.correction.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            submitCorrection();
          }}
        >
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="observation-id">
                {t("dataQuality.correction.observationId")}
              </FieldLabel>
              <Input
                id="observation-id"
                onChange={(event) => {
                  setObservationId(event.target.value);
                }}
                required
                value={observationId}
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
                  setExpectedVersion(event.target.value);
                }}
                required
                type="number"
                value={expectedVersion}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="correction-reason">
                {t("dataQuality.correction.reason")}
              </FieldLabel>
              <Input
                id="correction-reason"
                onChange={(event) => {
                  setReason(event.target.value);
                }}
                required
                value={reason}
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
                    setGenerationPowerWatts(event.target.value);
                  }}
                  type="number"
                  value={generationPowerWatts}
                />
              </Field>
            )}
          </FieldGroup>
          <Toggle
            onPressedChange={setDeleteMode}
            pressed={deleteMode}
            variant="outline"
          >
            {t("dataQuality.correction.deleteToggle")}
          </Toggle>
          <Button disabled={mutation.isPending} type="submit">
            {t("dataQuality.correction.submit")}
          </Button>
          {mutation.isSuccess ? (
            <Alert>
              <AlertTitle>
                {t("dataQuality.correction.successTitle")}
              </AlertTitle>
              <AlertDescription>
                {t("dataQuality.correction.successDescription")}
              </AlertDescription>
            </Alert>
          ) : null}
          {mutation.isError ? (
            <Alert variant="destructive">
              <AlertTitle>{t("dataQuality.correction.errorTitle")}</AlertTitle>
              <AlertDescription>
                {mutation.error.message === "correction_conflict"
                  ? t("dataQuality.correction.conflictDescription")
                  : t("dataQuality.correction.errorDescription")}
              </AlertDescription>
            </Alert>
          ) : null}
        </form>
      </CardContent>
    </Card>
  );
}
