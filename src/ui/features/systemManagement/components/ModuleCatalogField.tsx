import { useSolarModuleCatalog } from "@/features/equipmentCatalog";
import type { SolarModuleCatalogEntry } from "@/features/equipmentCatalog";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { CatalogCombobox, type CatalogComboboxOption } from "./CatalogCombobox";

/** Searchable solar-module catalog field with manual fallback. */
export function ModuleCatalogField({
  id,
  value,
  onSelect,
}: {
  id: string;
  value: string;
  onSelect: (entry: SolarModuleCatalogEntry | null, value: string) => void;
}) {
  const { i18n, t } = useTranslation();
  const [search, setSearch] = useState("");
  const catalog = useSolarModuleCatalog({ search, limit: 25 });
  const coefficientFormatter = new Intl.NumberFormat(i18n.language, {
    maximumFractionDigits: 2,
  });
  const catalogOptions: CatalogComboboxOption<SolarModuleCatalogEntry>[] = (
    catalog.data?.items ?? []
  ).map((entry) => {
    const label = `${entry.manufacturer} ${entry.model}`;
    return {
      id: entry.id,
      label,
      description: t("systemManagement.wizard.moduleOptionDetails", {
        power: entry.specification.peakPowerWatts,
        temperatureCoefficient: coefficientFormatter.format(
          entry.specification.peakPowerTemperatureCoefficientPpmPerCelsius /
            10_000,
        ),
      }),
      entry,
    };
  });
  const options: CatalogComboboxOption<SolarModuleCatalogEntry>[] = [
    {
      id: "manual",
      label: t("systemManagement.wizard.manualEntry"),
      description: t("systemManagement.wizard.manualModuleDescription"),
      entry: null,
      manual: true,
    },
    ...catalogOptions,
  ];

  return (
    <CatalogCombobox
      currentLabel={value}
      id={id}
      label={t("systemManagement.wizard.moduleCatalog")}
      onSelect={(option) => {
        onSelect(option.entry, option.entry ? option.label : "");
      }}
      onSearchChange={setSearch}
      options={options}
      placeholder={t("systemManagement.wizard.modulePlaceholder")}
      searchPlaceholder={t("systemManagement.wizard.moduleSearchPlaceholder")}
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
