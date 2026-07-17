import { InverterCatalogSelector } from "@/features/equipmentCatalog/components/InverterCatalogSelector";
import { SolarModuleCatalogSelector } from "@/features/equipmentCatalog/components/SolarModuleCatalogSelector";
import { useSaveEquipmentConfiguration } from "@/features/equipmentCatalog/hooks/useEquipmentCatalog";
import type {
  InverterCatalogEntry,
  SolarModuleCatalogEntry,
} from "@/features/equipmentCatalog/types/equipmentCatalog.types";
import { Button, Input, Label } from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

function snapshot(
  entry: InverterCatalogEntry | SolarModuleCatalogEntry,
): Record<string, unknown> {
  const common = {
    manufacturer: entry.manufacturer,
    model: entry.model,
    template: {
      entryId: entry.id,
      revision: entry.revision,
      valueProvenance: "catalog_copied",
    },
  };
  return "dc" in entry
    ? { ...common, dc: entry.dc, ac: entry.ac, operational: entry.operational }
    : { ...common, specification: entry.specification };
}

/** Persists manually entered or fully editable catalog-prefilled snapshots. @param props - Active account and system. @returns The confirmation form. */
export function EquipmentConfirmationForm({
  accountId,
  systemId,
}: {
  accountId: string;
  systemId: string;
}) {
  const { t } = useTranslation();
  const save = useSaveEquipmentConfiguration(accountId, systemId);
  const [inverter, setInverter] = useState<InverterCatalogEntry | null>(null);
  const [module, setModule] = useState<SolarModuleCatalogEntry | null>(null);
  const [inverterJson, setInverterJson] = useState("");
  const [moduleJson, setModuleJson] = useState("");
  const [count, setCount] = useState(1);
  const chooseInverter = (entry: InverterCatalogEntry) => {
    setInverter(entry);
    setInverterJson(JSON.stringify(snapshot(entry), null, 2));
  };
  const chooseModule = (entry: SolarModuleCatalogEntry) => {
    setModule(entry);
    setModuleJson(JSON.stringify(snapshot(entry), null, 2));
  };
  const submit = () => {
    try {
      const inverterValue = inverterJson
        ? (JSON.parse(inverterJson) as Record<string, unknown>)
        : null;
      const moduleValue = moduleJson
        ? (JSON.parse(moduleJson) as Record<string, unknown>)
        : null;
      const specification = moduleValue?.specification as
        Record<string, unknown> | undefined;
      const watts = Number(specification?.peakPowerWatts ?? 1);
      const now = Date.now();
      const inverterProvenance = inverter
        ? inverterJson === JSON.stringify(snapshot(inverter), null, 2)
          ? "catalog_copied"
          : "catalog_customized"
        : "manual";
      const moduleProvenance = module
        ? moduleJson === JSON.stringify(snapshot(module), null, 2)
          ? "catalog_copied"
          : "catalog_customized"
        : "manual";
      const inverterName =
        typeof inverterValue?.model === "string"
          ? inverterValue.model
          : t("features.equipmentCatalog.customInverter");
      save.mutate({
        name: inverterName,
        manufacturer: inverterValue?.manufacturer ?? null,
        model: inverterValue?.model ?? null,
        ratedPowerWatts: inverter?.ac.ratedActivePowerWatts ?? count * watts,
        valueProvenance: inverterProvenance,
        specificationSnapshot: inverterValue,
        effectiveFrom: now,
        strings: [
          {
            name: t("features.equipmentCatalog.defaultString"),
            panelCount: count,
            panelManufacturer:
              moduleValue?.manufacturer ??
              t("features.equipmentCatalog.customManufacturer"),
            panelModel:
              moduleValue?.model ?? t("features.equipmentCatalog.customModel"),
            valueProvenance: moduleProvenance,
            moduleSpecificationSnapshot: moduleValue,
            modulePeakPowerWatts: watts,
            effectiveFrom: now,
          },
        ],
      });
    } catch {
      save.reset();
    }
  };
  return (
    <section aria-labelledby="equipment-confirm-title" className="space-y-4">
      <h3 className="text-sm font-semibold" id="equipment-confirm-title">
        {t("features.equipmentCatalog.confirmTitle")}
      </h3>
      <div className="grid gap-6 lg:grid-cols-2">
        <InverterCatalogSelector
          onManual={() => {
            setInverter(null);
            setInverterJson("");
          }}
          onSelect={chooseInverter}
        />
        <SolarModuleCatalogSelector
          onManual={() => {
            setModule(null);
            setModuleJson("");
          }}
          onSelect={chooseModule}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="confirmed-inverter-json">
          {t("features.equipmentCatalog.inverterSnapshot")}
        </Label>
        <textarea
          className="min-h-40 w-full rounded-md border border-input bg-background p-3 font-mono text-xs focus-visible:ring-2 focus-visible:ring-ring"
          id="confirmed-inverter-json"
          onChange={(event) => {
            setInverterJson(event.target.value);
          }}
          value={inverterJson}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="confirmed-module-json">
          {t("features.equipmentCatalog.moduleSnapshot")}
        </Label>
        <textarea
          className="min-h-40 w-full rounded-md border border-input bg-background p-3 font-mono text-xs focus-visible:ring-2 focus-visible:ring-ring"
          id="confirmed-module-json"
          onChange={(event) => {
            setModuleJson(event.target.value);
          }}
          value={moduleJson}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="confirmed-module-count">
          {t("features.equipmentCatalog.moduleCount")}
        </Label>
        <Input
          id="confirmed-module-count"
          min={1}
          onChange={(event) => {
            setCount(Number(event.target.value));
          }}
          type="number"
          value={count}
        />
      </div>
      <p className="text-xs text-muted-foreground">
        {t("features.equipmentCatalog.snapshotGuidance")}
      </p>
      <p className="text-xs text-muted-foreground">
        {t("features.equipmentCatalog.provenanceLegend")}
      </p>
      <Button disabled={save.isPending} onClick={submit} type="button">
        {t("features.equipmentCatalog.save")}
      </Button>
      {save.isSuccess ? (
        <p aria-live="polite" className="text-sm">
          {t("features.equipmentCatalog.saved")}
        </p>
      ) : null}
      {save.isError ? (
        <p className="text-sm text-destructive" role="alert">
          {t("features.equipmentCatalog.validationError")}
        </p>
      ) : null}
    </section>
  );
}
