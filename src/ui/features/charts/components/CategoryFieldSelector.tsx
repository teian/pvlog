import type { SeriesField } from "@/features/charts/api/chartsApi";
import {
  CardDescription,
  ToggleGroup,
  ToggleGroupItem,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Category field selector properties. */
export interface CategoryFieldSelectorProps {
  /** Selectable fields of the category. */ fields: readonly SeriesField[];
  /** Called with the newly selected field. */ onChange: (
    field: SeriesField,
  ) => void;
  /** Currently selected field. */ value: SeriesField;
}

/** Renders the field toggle for a category, or nothing when the category exposes a single field. @param props - Selectable fields and current selection. @returns The field selector, or nothing. */
export function CategoryFieldSelector({
  fields,
  onChange,
  value,
}: CategoryFieldSelectorProps) {
  const { t } = useTranslation();
  if (fields.length <= 1) return null;
  return (
    <CardDescription>
      <ToggleGroup
        aria-label={t("charts.fieldLabel")}
        onValueChange={(next: string) => {
          if (next) onChange(next as SeriesField);
        }}
        size="sm"
        type="single"
        value={value}
        variant="outline"
      >
        {fields.map((option) => (
          <ToggleGroupItem key={option} value={option}>
            {t(`charts.field.${option}`)}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </CardDescription>
  );
}
