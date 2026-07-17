/* eslint-disable max-lines-per-function -- the string card keeps basic and disclosure-controlled advanced fields in one accessible form unit */
import type { SystemStringDraft } from "@/features/systemManagement/types/systemManagement.types";
import { ModuleCatalogField } from "@/features/systemManagement/components/ModuleCatalogField";
import {
  Button,
  Field,
  FieldGroup,
  FieldLabel,
  Input,
  Separator,
} from "@/shared/components";
import { ChevronDownIcon, PlusIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

const ORIENTATIONS = [
  [0, "north"],
  [45, "northEast"],
  [90, "east"],
  [135, "southEast"],
  [180, "south"],
  [225, "southWest"],
  [270, "west"],
  [315, "northWest"],
] as const;

/** Edits one PV string and computes its live peak capacity. @param props - String identity, draft, removal state and change callbacks. @returns Sectioned string editor card. */
export function StringDraftEditor({
  id,
  value,
  canRemove,
  onChange,
  onRemove,
}: {
  id: string;
  value: SystemStringDraft;
  canRemove: boolean;
  onChange: (value: SystemStringDraft) => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  const [advanced, setAdvanced] = useState(false);
  const kwp = (value.panelCount * value.modulePeakPowerWatts) / 1000;
  const patch = (next: Partial<SystemStringDraft>) => {
    onChange({ ...value, ...next });
  };
  return (
    <article className="flex flex-col gap-4 rounded-md border bg-card p-4 shadow-xs">
      <header className="flex items-center gap-3">
        <Input
          aria-label={t("systemManagement.wizard.stringName")}
          className="h-8 flex-1 font-semibold"
          onChange={(event) => {
            patch({ name: event.target.value });
          }}
          value={value.name}
        />
        <span className="font-mono text-sm font-semibold text-brand-foreground">
          {kwp.toFixed(2)} {t("systemManagement.units.kwp")}
        </span>
        <Button
          aria-label={t("systemManagement.actions.deleteString", {
            name: value.name,
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
      <ModuleCatalogField
        id={`${id}-module`}
        onSelect={(entry, manual) => {
          patch(
            entry
              ? {
                  panelManufacturer: entry.manufacturer,
                  panelModel: entry.model,
                  modulePeakPowerWatts: entry.specification.peakPowerWatts,
                  moduleSpecificationSnapshot: {
                    manufacturer: entry.manufacturer,
                    model: entry.model,
                    specification: entry.specification,
                    template: {
                      entryId: entry.id,
                      revision: entry.revision,
                      valueProvenance: "catalog_copied",
                    },
                  },
                }
              : { panelModel: manual, moduleSpecificationSnapshot: null },
          );
        }}
        value={[value.panelManufacturer, value.panelModel]
          .filter(Boolean)
          .join(" ")}
      />
      <FieldGroup className="grid gap-3 sm:grid-cols-4">
        <Field>
          <FieldLabel htmlFor={`${id}-count`}>
            {t("systemManagement.wizard.moduleCount")}
          </FieldLabel>
          <Input
            id={`${id}-count`}
            min={1}
            onChange={(event) => {
              patch({ panelCount: Number(event.target.value) });
            }}
            type="number"
            value={value.panelCount === 0 ? "" : value.panelCount}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-wp`}>
            {t("systemManagement.wizard.modulePower")}
          </FieldLabel>
          <Input
            id={`${id}-wp`}
            min={1}
            onChange={(event) => {
              patch({ modulePeakPowerWatts: Number(event.target.value) });
            }}
            type="number"
            value={
              value.modulePeakPowerWatts === 0 ? "" : value.modulePeakPowerWatts
            }
          />
        </Field>
        <Field className="sm:col-span-2">
          <FieldLabel htmlFor={`${id}-orientation`}>
            {t("systemManagement.wizard.orientation")}
          </FieldLabel>
          <select
            className="h-9 rounded-md border border-input bg-background px-3 text-sm"
            id={`${id}-orientation`}
            onChange={(event) => {
              patch({ orientationDegrees: Number(event.target.value) });
            }}
            value={value.orientationDegrees}
          >
            {ORIENTATIONS.map(([degrees, key]) => (
              <option key={degrees} value={degrees}>
                {t(`systemManagement.orientation.${key}`)}
              </option>
            ))}
          </select>
        </Field>
      </FieldGroup>
      <Button
        className="self-start"
        onClick={() => {
          setAdvanced((current) => !current);
        }}
        size="sm"
        type="button"
        variant="ghost"
      >
        <ChevronDownIcon
          className={advanced ? "rotate-180" : undefined}
          data-icon="inline-start"
        />
        {t("systemManagement.wizard.advanced")}
      </Button>
      {advanced ? (
        <div className="flex flex-col gap-4">
          <Separator />
          <FieldGroup className="grid gap-3 sm:grid-cols-3">
            <Field>
              <FieldLabel htmlFor={`${id}-azimuth`}>
                {t("systemManagement.wizard.azimuth")}
              </FieldLabel>
              <Input
                id={`${id}-azimuth`}
                max={359}
                min={0}
                onChange={(event) => {
                  patch({ orientationDegrees: Number(event.target.value) });
                }}
                type="number"
                value={value.orientationDegrees}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor={`${id}-tilt`}>
                {t("systemManagement.wizard.tilt")}
              </FieldLabel>
              <Input
                id={`${id}-tilt`}
                max={90}
                min={0}
                onChange={(event) => {
                  patch({ tiltDegrees: Number(event.target.value) });
                }}
                type="number"
                value={value.tiltDegrees}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor={`${id}-temp`}>
                {t("systemManagement.wizard.temperatureCoefficient")}
              </FieldLabel>
              <Input
                id={`${id}-temp`}
                onChange={(event) => {
                  patch({ temperatureCoefficient: Number(event.target.value) });
                }}
                step="0.01"
                type="number"
                value={value.temperatureCoefficient}
              />
            </Field>
          </FieldGroup>
          <Button
            onClick={() => {
              patch({
                shading: [
                  ...value.shading,
                  {
                    id: crypto.randomUUID(),
                    from: "07:00",
                    to: "09:00",
                    label: "",
                    degree: 30,
                  },
                ],
              });
            }}
            size="sm"
            type="button"
            variant="outline"
          >
            <PlusIcon data-icon="inline-start" />
            {t("systemManagement.wizard.addShading")}
          </Button>
          {value.shading.map((shade) => (
            <div
              className="grid gap-2 rounded-md bg-muted/40 p-3 sm:grid-cols-[1fr_1fr_2fr_1fr_auto]"
              key={shade.id}
            >
              {(["from", "to", "label", "degree"] as const).map((field) => (
                <Input
                  aria-label={t(`systemManagement.wizard.shading.${field}`)}
                  key={field}
                  onChange={(event) => {
                    patch({
                      shading: value.shading.map((item) =>
                        item.id === shade.id
                          ? {
                              ...item,
                              [field]:
                                field === "degree"
                                  ? Number(event.target.value)
                                  : event.target.value,
                            }
                          : item,
                      ),
                    });
                  }}
                  type={
                    field === "degree"
                      ? "number"
                      : field === "label"
                        ? "text"
                        : "time"
                  }
                  value={shade[field]}
                />
              ))}
              <Button
                aria-label={t("systemManagement.actions.deleteShading")}
                onClick={() => {
                  patch({
                    shading: value.shading.filter(
                      (item) => item.id !== shade.id,
                    ),
                  });
                }}
                size="icon-sm"
                type="button"
                variant="ghost"
              >
                <Trash2Icon />
              </Button>
            </div>
          ))}
        </div>
      ) : null}
    </article>
  );
}
