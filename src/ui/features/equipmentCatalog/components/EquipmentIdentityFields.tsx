import { Input, Label } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Renders editable manufacturer and model identity fields. @param props - Stable prefix and optional prefilled identity. @returns Two labelled manual-capable inputs. */
export function EquipmentIdentityFields({
  idPrefix,
  manufacturer,
  model,
}: {
  idPrefix: string;
  manufacturer: string;
  model: string;
}) {
  const { t } = useTranslation();
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <div className="space-y-2">
        <Label htmlFor={`${idPrefix}-manufacturer`}>
          {t("features.equipmentCatalog.manufacturer")}
        </Label>
        <Input defaultValue={manufacturer} id={`${idPrefix}-manufacturer`} />
      </div>
      <div className="space-y-2">
        <Label htmlFor={`${idPrefix}-model`}>
          {t("features.equipmentCatalog.model")}
        </Label>
        <Input defaultValue={model} id={`${idPrefix}-model`} />
      </div>
    </div>
  );
}
