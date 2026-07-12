import { EquipmentIdentityFields } from "@/features/equipmentCatalog/components/EquipmentIdentityFields";
import { InverterCatalogSelector } from "@/features/equipmentCatalog/components/InverterCatalogSelector";
import { SolarModuleCatalogSelector } from "@/features/equipmentCatalog/components/SolarModuleCatalogSelector";
import type {
  InverterCatalogEntry,
  SolarModuleCatalogEntry,
} from "@/features/equipmentCatalog/types/equipmentCatalog.types";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Hosts optional equipment prefilling beside first-class editable manual fields. @returns The system equipment template panel. */
export function EquipmentCatalogPanel() {
  const { t } = useTranslation();
  const [inverter, setInverter] = useState<InverterCatalogEntry | null>(null);
  const [module, setModule] = useState<SolarModuleCatalogEntry | null>(null);
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("features.equipmentCatalog.title")}</CardTitle>
        <CardDescription>
          {t("features.equipmentCatalog.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-6 lg:grid-cols-2">
        <div className="space-y-4">
          <InverterCatalogSelector
            onManual={() => {
              setInverter(null);
            }}
            onSelect={setInverter}
          />
          <EquipmentIdentityFields
            idPrefix="inverter"
            key={inverter?.id ?? "manual-inverter"}
            manufacturer={inverter?.manufacturer ?? ""}
            model={inverter?.model ?? ""}
          />
        </div>
        <div className="space-y-4">
          <SolarModuleCatalogSelector
            onManual={() => {
              setModule(null);
            }}
            onSelect={setModule}
          />
          <EquipmentIdentityFields
            idPrefix="module"
            key={module?.id ?? "manual-module"}
            manufacturer={module?.manufacturer ?? ""}
            model={module?.model ?? ""}
          />
        </div>
      </CardContent>
    </Card>
  );
}
