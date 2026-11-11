import { ORIENTATIONS } from "@/features/systemManagement/utils/orientations";
import {
  Button,
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  Input,
} from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Values that describe a single array without naming its modules. */
export interface QuickEquipment {
  capacityWatts: number;
  orientationDegrees: number;
  tiltDegrees: number;
}

/** Describes a single-array system by capacity, orientation and tilt. @param props - Request state, the submit callback and the switch to detailed entry. @returns A three-field form for systems that need no module details. */
export function EquipmentQuickEntry({
  pending,
  onSubmit,
  onDetailed,
}: {
  pending: boolean;
  onSubmit: (value: QuickEquipment) => void;
  onDetailed: () => void;
}) {
  const { t } = useTranslation();
  const [capacity, setCapacity] = useState("");
  const [orientation, setOrientation] = useState(180);
  const [tilt, setTilt] = useState("30");
  const [attempted, setAttempted] = useState(false);
  const kilowatts = Number(capacity.trim().replace(",", "."));
  const capacityValid = Number.isFinite(kilowatts) && kilowatts > 0;
  const tiltValue = Number(tilt);
  const tiltValid =
    Number.isInteger(tiltValue) && tiltValue >= 0 && tiltValue <= 90;
  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    setAttempted(true);
    if (capacityValid && tiltValid)
      onSubmit({
        capacityWatts: Math.round(kilowatts * 1_000),
        orientationDegrees: orientation,
        tiltDegrees: tiltValue,
      });
  }
  return (
    <form
      className="flex flex-col gap-4 rounded-md border p-4"
      onSubmit={submit}
    >
      <div>
        <h2 className="text-sm font-bold">{t("equipment.quick.title")}</h2>
        <p className="text-sm text-muted-foreground">
          {t("equipment.quick.description")}
        </p>
      </div>
      <FieldGroup className="grid gap-3 sm:grid-cols-3">
        <Field data-invalid={attempted && !capacityValid}>
          <FieldLabel htmlFor="quick-capacity">
            {t("setup.system.capacity")}
          </FieldLabel>
          <Input
            aria-invalid={attempted && !capacityValid}
            id="quick-capacity"
            inputMode="decimal"
            onChange={(event) => {
              setCapacity(event.target.value);
            }}
            value={capacity}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor="quick-orientation">
            {t("systemManagement.wizard.orientation")}
          </FieldLabel>
          <select
            className="h-9 rounded-md border border-input bg-transparent px-3 text-sm"
            id="quick-orientation"
            onChange={(event) => {
              setOrientation(Number(event.target.value));
            }}
            value={orientation}
          >
            {ORIENTATIONS.map(([degrees, key]) => (
              <option key={key} value={degrees}>
                {t(`systemManagement.orientation.${key}`)}
              </option>
            ))}
          </select>
        </Field>
        <Field data-invalid={attempted && !tiltValid}>
          <FieldLabel htmlFor="quick-tilt">
            {t("systemManagement.wizard.tilt")}
          </FieldLabel>
          <Input
            aria-invalid={attempted && !tiltValid}
            id="quick-tilt"
            inputMode="numeric"
            onChange={(event) => {
              setTilt(event.target.value);
            }}
            value={tilt}
          />
        </Field>
      </FieldGroup>
      {attempted && !(capacityValid && tiltValid) ? (
        <FieldDescription className="text-destructive" role="alert">
          {t("equipment.quick.invalid")}
        </FieldDescription>
      ) : null}
      <div className="flex flex-wrap gap-3">
        <Button disabled={pending} type="submit">
          {pending ? t("equipment.saving") : t("equipment.quick.save")}
        </Button>
        <Button onClick={onDetailed} type="button" variant="outline">
          {t("equipment.quick.detailed")}
        </Button>
      </div>
    </form>
  );
}
