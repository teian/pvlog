import {
  capacityWatts,
  type StringDraft,
} from "@/entities/equipmentDiscovery/adoptionDraft";
import { Field, FieldLabel, Input } from "@/shared/components";
import { useId } from "react";
import { useTranslation } from "react-i18next";

/** Asks for the name and capacity of one string of an adoption. @param props - The string, whether a missing capacity is to be marked, and the change callback. @returns One row with the DC input, a name field and a capacity field. */
export function DiscoveryStringFields({
  string,
  showInvalid,
  onChange,
}: {
  string: StringDraft;
  showInvalid: boolean;
  onChange: (patch: Partial<StringDraft>) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  return (
    <div className="grid items-end gap-3 sm:grid-cols-[8rem_1fr_10rem]">
      <span className="font-mono text-sm text-muted-foreground sm:pb-2">
        {string.tracker === null
          ? t("discovery.withoutTracker")
          : t("discovery.trackerLabel", { number: string.tracker })}
      </span>
      <Field>
        <FieldLabel htmlFor={`${id}-name`}>
          {t("discovery.stringName")}
        </FieldLabel>
        <Input
          id={`${id}-name`}
          onChange={(event) => {
            onChange({ name: event.target.value });
          }}
          value={string.name}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-capacity`}>
          {t("discovery.capacity")}
        </FieldLabel>
        <Input
          aria-invalid={showInvalid && capacityWatts(string.capacity) === null}
          id={`${id}-capacity`}
          inputMode="decimal"
          onChange={(event) => {
            onChange({ capacity: event.target.value });
          }}
          value={string.capacity}
        />
      </Field>
    </div>
  );
}
