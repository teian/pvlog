/* eslint-disable max-lines-per-function -- one form owns the equipment draft, its validation message and both entry modes */
import { EquipmentQuickEntry } from "@/features/systemManagement/components/EquipmentQuickEntry";
import { InverterDraftEditor } from "@/features/systemManagement/components/InverterDraftEditor";
import type {
  ManagedInverter,
  SystemInverterDraft,
} from "@/features/systemManagement/types/systemManagement.types";
import {
  emptyInverter,
  equipmentDrafts,
  equipmentIssue,
} from "@/features/systemManagement/utils/systemManagementDraft";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
} from "@/shared/components";
import { PlusIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** State of the save request, owned by the caller so it survives a reload of the equipment. */
export interface EquipmentSaveState {
  pending: boolean;
  failed: boolean;
  succeeded: boolean;
}

/** Edits the inverters and strings of a system. @param props - The stored inverters, whether they may only be viewed, the save state and the save callback. @returns Quick entry for a system without equipment, otherwise the detailed editor. */
export function SystemEquipmentForm({
  inverters,
  readOnly,
  save,
  onSave,
}: {
  inverters: ManagedInverter[];
  readOnly: boolean;
  save: EquipmentSaveState;
  onSave: (drafts: SystemInverterDraft[]) => void;
}) {
  const { t } = useTranslation();
  const [drafts, setDrafts] = useState(() => equipmentDrafts(inverters));
  const [detailed, setDetailed] = useState(inverters.length > 0);
  const issue = equipmentIssue(drafts);
  const store = onSave;

  if (!detailed && drafts.length === 0)
    return readOnly ? (
      <Alert>
        <AlertTitle>{t("equipment.empty.title")}</AlertTitle>
        <AlertDescription>{t("equipment.empty.description")}</AlertDescription>
      </Alert>
    ) : (
      <div className="flex flex-col gap-3">
        <EquipmentQuickEntry
          onDetailed={() => {
            setDrafts([emptyInverter()]);
            setDetailed(true);
          }}
          onSubmit={(quick) => {
            const inverter = emptyInverter();
            store([
              {
                ...inverter,
                name: t("equipment.quick.inverterName"),
                strings: inverter.strings.map((string) => ({
                  ...string,
                  name: t("equipment.quick.stringName"),
                  panelCount: 1,
                  modulePeakPowerWatts: quick.capacityWatts,
                  orientationDegrees: quick.orientationDegrees,
                  tiltDegrees: quick.tiltDegrees,
                })),
              },
            ]);
          }}
          pending={save.pending}
        />
        {save.failed ? (
          <p className="text-sm text-destructive" role="alert">
            {t("equipment.errors.save")}
          </p>
        ) : null}
      </div>
    );

  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (issue === null) store(drafts);
      }}
    >
      <fieldset className="contents" disabled={readOnly}>
        {drafts.map((inverter, index) => (
          <InverterDraftEditor
            canRemove
            index={index}
            key={inverter.id ?? `new-${String(index)}`}
            onChange={(next) => {
              setDrafts(
                drafts.map((value, position) =>
                  position === index ? next : value,
                ),
              );
            }}
            onRemove={() => {
              setDrafts(drafts.filter((_, position) => position !== index));
            }}
            value={inverter}
          />
        ))}
      </fieldset>
      {readOnly ? null : (
        <>
          <Button
            className="self-start border-dashed"
            onClick={() => {
              setDrafts([...drafts, emptyInverter(drafts.length + 1)]);
            }}
            type="button"
            variant="outline"
          >
            <PlusIcon data-icon="inline-start" />
            {t("systemManagement.wizard.addInverter")}
          </Button>
          {issue ? (
            <p className="text-sm text-destructive" role="alert">
              {t(`equipment.issues.${issue.kind}`, issue)}
            </p>
          ) : null}
          {save.failed ? (
            <p className="text-sm text-destructive" role="alert">
              {t("equipment.errors.save")}
            </p>
          ) : null}
          {save.succeeded ? (
            <p className="text-sm text-success" role="status">
              {t("equipment.saved")}
            </p>
          ) : null}
          <Button
            className="self-start"
            disabled={save.pending || issue !== null}
            type="submit"
          >
            {save.pending ? t("equipment.saving") : t("equipment.save")}
          </Button>
        </>
      )}
    </form>
  );
}
