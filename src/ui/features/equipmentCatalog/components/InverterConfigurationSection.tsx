import { EquipmentIdentityFields } from "@/features/equipmentCatalog/components/EquipmentIdentityFields";
import { InverterCatalogSelector } from "@/features/equipmentCatalog/components/InverterCatalogSelector";
import { TechnicalDataTable } from "@/features/equipmentCatalog/components/TechnicalDataTable";
import type { InverterCatalogEntry } from "@/features/equipmentCatalog/types/equipmentCatalog.types";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Combines optional inverter prefilling, editable identity, and technical review. @returns The inverter configuration section. */
export function InverterConfigurationSection() {
  const { t } = useTranslation();
  const [entry, setEntry] = useState<InverterCatalogEntry | null>(null);
  const rows = entry
    ? ([
        [t("features.equipmentCatalog.fields.revision"), entry.revision],
        [
          t("features.equipmentCatalog.fields.topology"),
          entry.dc.topology ?? "—",
        ],
        [
          t("features.equipmentCatalog.fields.mppts"),
          String(entry.dc.mpptInputs.length),
        ],
        [
          t("features.equipmentCatalog.fields.strings"),
          String(entry.dc.totalStringInputCount),
        ],
        [
          t("features.equipmentCatalog.fields.mpptAllocation"),
          entry.dc.mpptInputs
            .map(
              (tracker) =>
                `${String(tracker.trackerIndex)}: ${String(tracker.stringInputCount)}`,
            )
            .join(", "),
        ],
        [
          t("features.equipmentCatalog.fields.dcVoltage"),
          `${String((entry.dc.minimumMpptVoltageMillivolts ?? 0) / 1000)}–${String((entry.dc.maximumMpptVoltageMillivolts ?? 0) / 1000)} V`,
        ],
        [
          t("features.equipmentCatalog.fields.dcCurrent"),
          `${String((entry.dc.maximumInputCurrentMilliamperes ?? 0) / 1000)} A`,
        ],
        [
          t("features.equipmentCatalog.fields.acPower"),
          `${String(entry.ac.ratedActivePowerWatts)} W`,
        ],
        [
          t("features.equipmentCatalog.fields.phases"),
          String(entry.ac.phaseCount),
        ],
        [
          t("features.equipmentCatalog.fields.communications"),
          entry.operational.communicationInterfaces.join(", ") || "—",
        ],
        [
          t("features.equipmentCatalog.fields.efficiency"),
          entry.operational.maximumEfficiencyBasisPoints
            ? `${String(entry.operational.maximumEfficiencyBasisPoints / 100)} %`
            : "—",
        ],
        [
          t("features.equipmentCatalog.fields.temperature"),
          entry.operational.operatingTemperature
            ? `${String(entry.operational.operatingTemperature.minimumMilliCelsius / 1000)}–${String(entry.operational.operatingTemperature.maximumMilliCelsius / 1000)} °C`
            : "—",
        ],
        [
          t("features.equipmentCatalog.fields.dimensions"),
          entry.operational.dimensionsMillimetres
            ? `${String(entry.operational.dimensionsMillimetres.length)} × ${String(entry.operational.dimensionsMillimetres.width)} × ${String(entry.operational.dimensionsMillimetres.height)} mm`
            : "—",
        ],
        [
          t("features.equipmentCatalog.fields.weight"),
          entry.operational.weightGrams
            ? `${String(entry.operational.weightGrams / 1000)} kg`
            : "—",
        ],
        [
          t("features.equipmentCatalog.fields.source"),
          entry.provenance.sourceName,
        ],
      ] as const)
    : [];
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
