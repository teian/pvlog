import type {
  BindableInverter,
  ChannelTargetKind,
} from "@/features/measurementChannels/measurementChannels.types";
import { useTranslation } from "react-i18next";

/** Encodes a binding as one option value. @param kind - Target kind. @param id - Target identifier, when the kind needs one. @returns A value unique among the options. */
export function targetValue(kind: ChannelTargetKind, id: string | null) {
  return id ? `${kind}:${id}` : kind;
}

/** Decodes an option value back into a binding. @param value - Value produced by targetValue. @returns Target kind and identifier. */
export function parseTarget(value: string): {
  targetKind: ChannelTargetKind;
  targetId: string | null;
} {
  const [kind, id] = value.split(":");
  return {
    targetKind: kind as ChannelTargetKind,
    targetId: id ?? null,
  };
}

/** Lets the user choose what a channel measures. @param props - Accessible name, current binding, the system's equipment and the change callback. @returns A select grouped by inverter. */
export function ChannelTargetSelect({
  label,
  value,
  equipment,
  disabled,
  onChange,
}: {
  label: string;
  value: string;
  equipment: BindableInverter[];
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <select
      aria-label={label}
      className="h-9 max-w-full rounded-md border border-input bg-transparent px-3 text-sm"
      disabled={disabled}
      onChange={(event) => {
        onChange(event.target.value);
      }}
      value={value}
    >
      <option value="unassigned">{t("channels.targets.unassigned")}</option>
      <option value="plant">{t("channels.targets.plant")}</option>
      {equipment.map((inverter) => (
        <optgroup key={inverter.id} label={inverter.name}>
          <option value={targetValue("inverter", inverter.id)}>
            {t("channels.targets.inverter", { name: inverter.name })}
          </option>
          {inverter.strings.map((string) => (
            <option key={string.id} value={targetValue("string", string.id)}>
              {t("channels.targets.string", { name: string.name })}
            </option>
          ))}
        </optgroup>
      ))}
    </select>
  );
}
