import { InverterConfigurationSection } from "@/features/equipmentCatalog/components/InverterConfigurationSection";
import { SolarModuleConfigurationSection } from "@/features/equipmentCatalog/components/SolarModuleConfigurationSection";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Hosts optional equipment prefilling beside first-class editable manual fields. @returns The system equipment template panel. */
export function EquipmentCatalogPanel() {
  const { t } = useTranslation();
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("features.equipmentCatalog.title")}</CardTitle>
        <CardDescription>
          {t("features.equipmentCatalog.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="grid gap-6 lg:grid-cols-2">
        <InverterConfigurationSection />
        <SolarModuleConfigurationSection />
      </CardContent>
    </Card>
  );
}
