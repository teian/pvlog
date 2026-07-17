import { useInverterCatalog } from "@/features/equipmentCatalog";
import type { InverterCatalogEntry } from "@/features/equipmentCatalog";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { CatalogCombobox, type CatalogComboboxOption } from "./CatalogCombobox";

/** Searchable inverter catalog field with manual fallback. */
export function InverterCatalogField({
  id,
  value,
  onSelect,
}: {
  id: string;
  value: string;
  onSelect: (entry: InverterCatalogEntry | null, value: string) => void;
}) {
  const { t } = useTranslation();
  const [search, setSearch] = useState("");
  const catalog = useInverterCatalog({ search, limit: 25 });
  const catalogOptions: CatalogComboboxOption<InverterCatalogEntry>[] = (
    catalog.data?.items ?? []
  ).map((entry) => {
    const label = `${entry.manufacturer} ${entry.model}`;
    return {
      id: entry.id,
      label,
      description: t("systemManagement.wizard.inverterOptionDetails", {
        power: entry.ac.ratedActivePowerWatts,
        mppts: entry.dc.mpptInputs.length,
        phases: entry.ac.phaseCount,
      }),
      entry,
    };
  });
  const options: CatalogComboboxOption<InverterCatalogEntry>[] = [
    {
      id: "manual",
      label: t("systemManagement.wizard.manualEntry"),
      description: t("systemManagement.wizard.manualInverterDescription"),
      entry: null,
      manual: true,
    },
    ...catalogOptions,
  ];

  return (
    <CatalogCombobox
      currentLabel={value}
      id={id}
      label={t("systemManagement.wizard.inverterCatalog")}
      onSelect={(option) => {
        onSelect(option.entry, option.entry ? option.label : "");
      }}
      onSearchChange={setSearch}
      options={options}
      placeholder={t("systemManagement.wizard.catalogPlaceholder")}
      searchPlaceholder={t("systemManagement.wizard.inverterSearchPlaceholder")}
      statusText={
        catalog.isLoading
          ? t("systemManagement.wizard.catalogLoading")
          : catalogOptions.length === 0
            ? t("systemManagement.wizard.noCatalogMatches")
            : undefined
      }
    />
  );
}
