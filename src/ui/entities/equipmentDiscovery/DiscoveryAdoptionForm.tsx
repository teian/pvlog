import {
  initialStrings,
  toAdoption,
  type StringDraft,
} from "@/entities/equipmentDiscovery/adoptionDraft";
import { DiscoveryStringFields } from "@/entities/equipmentDiscovery/DiscoveryStringFields";
import type {
  DiscoveryAdoption,
  DiscoveryProposal,
} from "@/entities/equipmentDiscovery/equipmentDiscovery.types";
import {
  Button,
  Field,
  FieldDescription,
  FieldLabel,
  Input,
} from "@/shared/components";
import { useId, useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Asks for what adopting a device needs: a name for the inverter, and a name and capacity for each string. @param props - The proposal, whether a request runs, and the adopt and dismiss callbacks. @returns A form with one row per DC input that has delivered power. */
export function DiscoveryAdoptionForm({
  proposal,
  busy,
  onAdopt,
  onDismiss,
}: {
  proposal: DiscoveryProposal;
  busy: boolean;
  onAdopt: (adoption: DiscoveryAdoption) => void;
  onDismiss: () => void;
}) {
  const { t } = useTranslation();
  const nameId = useId();
  const [name, setName] = useState(
    proposal.reportedName ??
      t("discovery.inverterTitle", { serial: proposal.deviceSerial }),
  );
  const [strings, setStrings] = useState<StringDraft[]>(() =>
    initialStrings(proposal, t),
  );
  const [attempted, setAttempted] = useState(false);
  const adoption = toAdoption(name, strings);
  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    setAttempted(true);
    if (adoption) onAdopt(adoption);
  }
  return (
    <form className="flex flex-col gap-4" onSubmit={submit}>
      <Field className="max-w-sm">
        <FieldLabel htmlFor={nameId}>{t("discovery.inverterName")}</FieldLabel>
        <Input
          id={nameId}
          onChange={(event) => {
            setName(event.target.value);
          }}
          value={name}
        />
      </Field>
      <fieldset className="flex flex-col gap-3">
        <legend className="text-sm font-medium">
          {t("discovery.stringsLegend")}
        </legend>
        <p className="text-sm text-muted-foreground">
          {proposal.trackers.length > 0
            ? t("discovery.stringsHint")
            : t("discovery.noTrackers")}
        </p>
        {strings.map((string, index) => (
          <DiscoveryStringFields
            key={string.tracker ?? "single"}
            onChange={(patch) => {
              setStrings((current) =>
                current.map((entry, position) =>
                  position === index ? { ...entry, ...patch } : entry,
                ),
              );
            }}
            showInvalid={attempted}
            string={string}
          />
        ))}
      </fieldset>
      {attempted && !adoption ? (
        <FieldDescription className="text-destructive" role="alert">
          {t("discovery.invalid")}
        </FieldDescription>
      ) : null}
      <div className="flex flex-wrap gap-3">
        <Button disabled={busy} type="submit">
          {busy ? t("discovery.adopting") : t("discovery.adopt")}
        </Button>
        <Button
          disabled={busy}
          onClick={onDismiss}
          type="button"
          variant="outline"
        >
          {t("discovery.dismiss")}
        </Button>
      </div>
    </form>
  );
}
