import {
  type CorrectionDraft,
  CorrectionFields,
} from "@/features/dataQuality/components/CorrectionFields";
import { CorrectionResultAlert } from "@/features/dataQuality/components/CorrectionResultAlert";
import { useCorrectObservation } from "@/features/dataQuality/hooks/useCorrectObservation";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Toggle,
} from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

const EMPTY_DRAFT: CorrectionDraft = {
  expectedVersion: "",
  generationPowerWatts: "",
  observationId: "",
  reason: "",
};

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
  const [draft, setDraft] = useState<CorrectionDraft>(EMPTY_DRAFT);
  const [deleteMode, setDeleteMode] = useState(false);

  const patch = (next: Partial<CorrectionDraft>) => {
    setDraft((value) => ({ ...value, ...next }));
  };

  const submitCorrection = () => {
    mutation.mutate(
      {
        systemId,
        observationId: draft.observationId,
        expectedVersion: Number(draft.expectedVersion),
        reason: draft.reason,
        delete: deleteMode,
        ...(deleteMode || draft.generationPowerWatts === ""
          ? {}
          : { generationPowerWatts: Number(draft.generationPowerWatts) }),
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
          <CorrectionFields
            deleteMode={deleteMode}
            draft={draft}
            onChange={patch}
          />
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
          <CorrectionResultAlert
            error={mutation.error}
            success={mutation.isSuccess}
          />
        </form>
      </CardContent>
    </Card>
  );
}
