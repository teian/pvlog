import { useSolarModuleCatalog } from "@/features/equipmentCatalog/hooks/useEquipmentCatalog";
import type { SolarModuleCatalogEntry } from "@/features/equipmentCatalog/types/equipmentCatalog.types";
import { Button, Input, Label } from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Optional module-template picker with an always-available manual path. @param props - Selection and manual-entry callbacks. @returns An accessible catalog search control. */
export function SolarModuleCatalogSelector({
  onManual,
  onSelect,
}: {
  onManual: () => void;
  onSelect: (entry: SolarModuleCatalogEntry) => void;
}) {
  const { t } = useTranslation();
  const [search, setSearch] = useState("");
  const catalog = useSolarModuleCatalog({ search, limit: 25 });
  const items = catalog.data?.items ?? [];
  return (
    <section aria-labelledby="module-catalog-title" className="space-y-3">
      <h3 className="text-sm font-semibold" id="module-catalog-title">
        {t("features.equipmentCatalog.moduleTitle")}
      </h3>
      <div className="space-y-2">
        <Label htmlFor="module-catalog-search">
          {t("features.equipmentCatalog.search")}
        </Label>
        <Input
          id="module-catalog-search"
          onChange={(event) => {
            setSearch(event.target.value);
          }}
          placeholder={t("features.equipmentCatalog.modulePlaceholder")}
          value={search}
        />
      </div>
      {catalog.isPending ? (
        <p aria-live="polite" className="text-sm text-muted-foreground">
          {t("features.equipmentCatalog.loading")}
        </p>
      ) : null}
      {catalog.isError ? (
        <p className="text-sm text-destructive" role="alert">
          {t("features.equipmentCatalog.error")}
        </p>
      ) : null}
      {!catalog.isPending && !catalog.isError && items.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {t("features.equipmentCatalog.empty")}
        </p>
      ) : null}
      {items.length > 0 ? (
        <div className="space-y-2">
          <Label htmlFor="module-catalog-results">
            {t("features.equipmentCatalog.results")}
          </Label>
          <select
            className="min-h-28 w-full rounded-md border border-input bg-background p-2 text-sm focus-visible:ring-2 focus-visible:ring-ring"
            id="module-catalog-results"
            onChange={(event) => {
              const entry = items.find(
                (item) => item.id === event.target.value,
              );
              if (entry) onSelect(entry);
            }}
            size={Math.min(5, items.length)}
          >
            <option value="">{t("features.equipmentCatalog.choose")}</option>
            {items.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {t("features.equipmentCatalog.moduleOption", {
                  manufacturer: entry.manufacturer,
                  model: entry.model,
                  watts: entry.specification.peakPowerWatts,
                })}
              </option>
            ))}
          </select>
        </div>
      ) : null}
      <Button onClick={onManual} type="button" variant="outline">
        {t("features.equipmentCatalog.manualModule")}
      </Button>
    </section>
  );
}
