/* eslint-disable max-lines-per-function -- the numbered inverter card keeps its closely related catalog and specification fields together */
import { InverterCatalogField } from "@/features/systemManagement/components/InverterCatalogField";
import { StringDraftEditor } from "@/features/systemManagement/components/StringDraftEditor";
import type { SystemInverterDraft } from "@/features/systemManagement/types/systemManagement.types";
import { emptyString } from "@/features/systemManagement/utils/systemManagementDraft";
import {
  Button,
  Field,
  FieldGroup,
  FieldLabel,
  Input,
  Separator,
} from "@/shared/components";
import { CpuIcon, PlusIcon, Trash2Icon } from "lucide-react";
import { useTranslation } from "react-i18next";

/** Edits one inverter. @param props - Position, draft, removal state and change callbacks. @returns Numbered inverter section. */
export function InverterDraftEditor({
  index,
  value,
  canRemove,
  onChange,
  onRemove,
}: {
  index: number;
  value: SystemInverterDraft;
  canRemove: boolean;
  onChange: (value: SystemInverterDraft) => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  const prefix = `inverter-${String(index)}`;
  const patch = (next: Partial<SystemInverterDraft>) => {
    onChange({ ...value, ...next });
  };
  return (
    <article className="flex flex-col gap-5 rounded-lg border bg-card p-4 shadow-xs">
      <header className="flex items-center gap-3">
        <span className="flex size-7 items-center justify-center rounded-sm bg-primary font-mono text-xs font-bold text-primary-foreground">
          {index + 1}
        </span>
        <CpuIcon aria-hidden="true" className="text-primary" />
        <h3 className="flex-1 text-sm font-bold">
          {t("systemManagement.wizard.inverterNumber", { number: index + 1 })}
        </h3>
        <Button
          aria-label={t("systemManagement.actions.deleteInverter", {
            number: index + 1,
          })}
          disabled={!canRemove}
          onClick={onRemove}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <Trash2Icon />
        </Button>
      </header>
      <InverterCatalogField
        id={`${prefix}-catalog`}
        onSelect={(entry, manual) => {
          patch(
            entry
              ? {
                  name: `${entry.manufacturer} ${entry.model}`,
                  manufacturer: entry.manufacturer,
                  model: entry.model,
                  ratedPowerWatts: entry.ac.ratedActivePowerWatts,
                  specificationSnapshot: {
                    manufacturer: entry.manufacturer,
                    model: entry.model,
                    dc: entry.dc,
                    ac: entry.ac,
                    operational: entry.operational,
                    template: {
                      entryId: entry.id,
                      revision: entry.revision,
                      valueProvenance: "catalog_copied",
                    },
                  },
                }
              : { name: manual, model: manual, specificationSnapshot: null },
          );
        }}
        value={[value.manufacturer, value.model].filter(Boolean).join(" ")}
      />
      <FieldGroup className="grid gap-3 sm:grid-cols-2">
        <Field>
          <FieldLabel htmlFor={`${prefix}-name`}>
            {t("systemManagement.wizard.modelName")}
          </FieldLabel>
          <Input
            id={`${prefix}-name`}
            onChange={(event) => {
              patch({ name: event.target.value, model: event.target.value });
            }}
            readOnly={Boolean(value.specificationSnapshot)}
            value={value.name}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${prefix}-power`}>
            {t("systemManagement.wizard.maxPower")}
          </FieldLabel>
          <Input
            id={`${prefix}-power`}
            min={0}
            onChange={(event) => {
              patch({ ratedPowerWatts: Number(event.target.value) });
            }}
            readOnly={Boolean(value.specificationSnapshot)}
            type="number"
            value={value.ratedPowerWatts === 0 ? "" : value.ratedPowerWatts}
          />
        </Field>
      </FieldGroup>
      {value.specificationSnapshot ? (
        <Button
          className="self-start"
          onClick={() => {
            patch({ specificationSnapshot: null });
          }}
          size="sm"
          type="button"
          variant="link"
        >
          {t("systemManagement.wizard.overrideCatalog")}
        </Button>
      ) : null}
      <Separator />
      <section className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <h4 className="text-xs font-bold text-muted-foreground">
            {t("systemManagement.wizard.strings")}
          </h4>
          <p className="text-xs text-muted-foreground">
            {t("systemManagement.wizard.stringsDescription")}
          </p>
        </div>
        {value.strings.map((string, stringIndex) => (
          <StringDraftEditor
            canRemove={value.strings.length > 1}
            id={`${prefix}-string-${String(stringIndex)}`}
            key={string.id ?? `string-${String(index)}-${String(stringIndex)}`}
            onChange={(next) => {
              patch({
                strings: value.strings.map((current, currentIndex) =>
                  currentIndex === stringIndex ? next : current,
                ),
              });
            }}
            onRemove={() => {
              patch({
                strings: value.strings.filter(
                  (_, currentIndex) => currentIndex !== stringIndex,
                ),
              });
            }}
            value={string}
          />
        ))}
        <Button
          className="self-start border-dashed"
          onClick={() => {
            patch({
              strings: [
                ...value.strings,
                emptyString(value.strings.length + 1),
              ],
            });
          }}
          size="sm"
          type="button"
          variant="outline"
        >
          <PlusIcon data-icon="inline-start" />
          {t("systemManagement.wizard.addString", { number: index + 1 })}
        </Button>
      </section>
    </article>
  );
}
