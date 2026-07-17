import {
  Combobox,
  ComboboxContent,
  ComboboxInput,
  ComboboxTrigger,
  Field,
  FieldLabel,
} from "@/shared/components";
import { cn } from "@/shared/lib/utils";
import { useMemo, useState } from "react";

import {
  CatalogComboboxOptions,
  type CatalogComboboxOption,
} from "./CatalogComboboxOptions";

export type { CatalogComboboxOption } from "./CatalogComboboxOptions";

interface CatalogComboboxProps<T> {
  id: string;
  label: string;
  placeholder: string;
  searchPlaceholder: string;
  options: CatalogComboboxOption<T>[];
  currentLabel: string;
  statusText?: string | undefined;
  onSearchChange: (search: string) => void;
  onSelect: (option: CatalogComboboxOption<T>) => void;
}

/** Searchable catalog selector with a persistent manual-entry option. */
export function CatalogCombobox<T>({
  id,
  label,
  placeholder,
  searchPlaceholder,
  options,
  currentLabel,
  statusText,
  onSearchChange,
  onSelect,
}: CatalogComboboxProps<T>) {
  const [search, setSearch] = useState("");
  const [open, setOpen] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = useMemo(
    () =>
      options.find((option) => option.id === selectedId) ??
      options.find((option) => option.label === currentLabel) ??
      null,
    [currentLabel, options, selectedId],
  );
  const labelId = `${id}-label`;

  return (
    <Field>
      <FieldLabel id={labelId} htmlFor={id}>
        {label}
      </FieldLabel>
      <Combobox
        autoHighlight
        filter={null}
        inputValue={search}
        isItemEqualToValue={(option, value) => option.id === value.id}
        itemToStringLabel={(option) => option.label}
        itemToStringValue={(option) => option.id}
        items={options}
        onInputValueChange={(nextSearch, details) => {
          if (
            details.reason === "input-change" ||
            details.reason === "input-clear"
          ) {
            setSearch(nextSearch);
            onSearchChange(nextSearch);
          }
        }}
        onOpenChange={(nextOpen) => {
          setOpen(nextOpen);
          if (!nextOpen) {
            setSearch("");
            onSearchChange("");
          }
        }}
        onValueChange={(option) => {
          if (!option) return;
          setSelectedId(option.id);
          setSearch("");
          onSearchChange("");
          onSelect(option);
        }}
        open={open}
        value={selected}
      >
        <ComboboxTrigger
          id={id}
          aria-labelledby={labelId}
          onClick={() => {
            setOpen(true);
          }}
        >
          <span
            className={cn(
              "block truncate",
              !selected && "text-muted-foreground",
            )}
          >
            {selected?.label ?? placeholder}
          </span>
        </ComboboxTrigger>
        <ComboboxContent aria-label={label}>
          <ComboboxInput
            aria-label={searchPlaceholder}
            placeholder={searchPlaceholder}
          />
          <CatalogComboboxOptions statusText={statusText} />
        </ComboboxContent>
      </Combobox>
    </Field>
  );
}
