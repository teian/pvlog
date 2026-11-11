import { useSystemOverviews } from "@/features/reporting/hooks/useReporting";
import {
  Combobox,
  ComboboxContent,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
  Field,
  FieldLabel,
} from "@/shared/components";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";

interface SystemOption {
  id: string;
  label: string;
}

interface SystemSelectorProps {
  systemIds: string[];
  selectedSystemId: string;
  onSelect: (systemId: string) => void;
}

/**
 * Lets the reader pick which system the surrounding report describes.
 *
 * Nothing renders for a single visible system, because the reports already
 * describe it unambiguously.
 */
export function SystemSelector({
  systemIds,
  selectedSystemId,
  onSelect,
}: SystemSelectorProps) {
  const { t } = useTranslation();
  const overviews = useSystemOverviews(systemIds);
  const options = useMemo<SystemOption[]>(
    () =>
      systemIds.map((systemId, index) => {
        const overview = overviews[index]?.data;
        const name = overview?.name ?? systemId;
        return {
          id: systemId,
          label:
            overview?.lifecycle === "archived"
              ? t("reporting.systemSelector.archived", { name })
              : name,
        };
      }),
    [overviews, systemIds, t],
  );
  const selected =
    options.find((option) => option.id === selectedSystemId) ?? null;

  if (systemIds.length < 2) {
    return null;
  }

  return (
    <Field className="max-w-sm">
      <FieldLabel htmlFor="reporting-system">
        {t("reporting.systemSelector.label")}
      </FieldLabel>
      <Combobox
        isItemEqualToValue={(option, value) => option.id === value.id}
        itemToStringLabel={(option) => option.label}
        itemToStringValue={(option) => option.id}
        items={options}
        onValueChange={(option) => {
          if (option) {
            onSelect(option.id);
          }
        }}
        value={selected}
      >
        <ComboboxTrigger id="reporting-system">
          {selected?.label ?? t("reporting.systemSelector.placeholder")}
        </ComboboxTrigger>
        <ComboboxContent aria-label={t("reporting.systemSelector.label")}>
          <ComboboxList>
            {options.map((option) => (
              <ComboboxItem key={option.id} value={option}>
                {option.label}
              </ComboboxItem>
            ))}
          </ComboboxList>
        </ComboboxContent>
      </Combobox>
    </Field>
  );
}
