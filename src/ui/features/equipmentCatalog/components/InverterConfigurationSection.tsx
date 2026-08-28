import { EquipmentIdentityFields } from "@/features/equipmentCatalog/components/EquipmentIdentityFields";
import { InverterCatalogSelector } from "@/features/equipmentCatalog/components/InverterCatalogSelector";
import { TechnicalDataTable } from "@/features/equipmentCatalog/components/TechnicalDataTable";
import type { InverterCatalogEntry } from "@/features/equipmentCatalog/types/equipmentCatalog.types";
import { inverterTechnicalRows } from "@/features/equipmentCatalog/utils/technicalDataRows";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Combines optional inverter prefilling, editable identity, and technical review. @returns The inverter configuration section. */
export function InverterConfigurationSection() {
  const { t } = useTranslation();
  const [entry, setEntry] = useState<InverterCatalogEntry | null>(null);
  const rows = entry === null ? [] : inverterTechnicalRows(entry, t);
  return (
    <div className="space-y-4">
      <InverterCatalogSelector
        onManual={() => {
          setEntry(null);
        }}
        onSelect={setEntry}
      />
      <EquipmentIdentityFields
        idPrefix="inverter"
        key={entry?.id ?? "manual-inverter"}
        manufacturer={entry?.manufacturer ?? ""}
        model={entry?.model ?? ""}
      />
      {entry ? (
        <TechnicalDataTable
          caption={t("features.equipmentCatalog.inverterData")}
          rows={rows}
        />
      ) : null}
    </div>
  );
}
