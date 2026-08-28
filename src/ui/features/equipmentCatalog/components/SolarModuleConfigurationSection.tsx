import { EquipmentIdentityFields } from "@/features/equipmentCatalog/components/EquipmentIdentityFields";
import { SolarModuleCatalogSelector } from "@/features/equipmentCatalog/components/SolarModuleCatalogSelector";
import { TechnicalDataTable } from "@/features/equipmentCatalog/components/TechnicalDataTable";
import type { SolarModuleCatalogEntry } from "@/features/equipmentCatalog/types/equipmentCatalog.types";
import { solarModuleTechnicalRows } from "@/features/equipmentCatalog/utils/technicalDataRows";
import { Input, Label } from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Combines optional module prefilling, editable string composition, and technical review. @returns The solar-module configuration section. */
export function SolarModuleConfigurationSection() {
  const { t } = useTranslation();
  const [entry, setEntry] = useState<SolarModuleCatalogEntry | null>(null);
  const [moduleCount, setModuleCount] = useState(1);
  const [peakPowerWatts, setPeakPowerWatts] = useState(0);
  const select = (selected: SolarModuleCatalogEntry) => {
    setEntry(selected);
    setPeakPowerWatts(selected.specification.peakPowerWatts);
  };
  const rows =
    entry === null
      ? []
      : solarModuleTechnicalRows(entry, moduleCount, peakPowerWatts, t);
  return (
    <div className="space-y-4">
      <SolarModuleCatalogSelector
        onManual={() => {
          setEntry(null);
          setPeakPowerWatts(0);
        }}
        onSelect={select}
      />
      <EquipmentIdentityFields
        idPrefix="module"
        key={entry?.id ?? "manual-module"}
        manufacturer={entry?.manufacturer ?? ""}
        model={entry?.model ?? ""}
      />
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-2">
          <Label htmlFor="module-count">
            {t("features.equipmentCatalog.moduleCount")}
          </Label>
          <Input
            id="module-count"
            min={1}
            onChange={(event) => {
              setModuleCount(Number(event.target.value));
            }}
            type="number"
            value={moduleCount}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="module-power">
            {t("features.equipmentCatalog.modulePower")}
          </Label>
          <Input
            id="module-power"
            min={1}
            onChange={(event) => {
              setPeakPowerWatts(Number(event.target.value));
            }}
            type="number"
            value={peakPowerWatts}
          />
        </div>
      </div>
      <p aria-live="polite" className="text-sm font-medium tabular-nums">
        {t("features.equipmentCatalog.totalPower", {
          watts: moduleCount * peakPowerWatts,
        })}
      </p>
      {entry ? (
        <TechnicalDataTable
          caption={t("features.equipmentCatalog.moduleData")}
          rows={rows}
        />
      ) : null}
    </div>
  );
}
