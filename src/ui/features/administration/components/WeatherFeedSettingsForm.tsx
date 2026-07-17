import { useSaveWeatherFeedSettings } from "@/features/administration/hooks/useAdministration";
import type { WeatherFeedSettings } from "@/features/administration/types/administration.types";
import { Button, Field, FieldLabel, Input, Switch } from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Edits and persists weather feed settings. */
export function WeatherFeedSettingsForm({
  initial,
}: {
  initial: WeatherFeedSettings;
}) {
  const { t } = useTranslation();
  const save = useSaveWeatherFeedSettings();
  const [endpoint, setEndpoint] = useState(initial.endpoint);
  const [secretRef, setSecretRef] = useState(initial.credentialSecretRef ?? "");
  const [enabled, setEnabled] = useState(initial.enabled);
  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    save.mutate({
      ...initial,
      enabled,
      endpoint,
      credentialSecretRef: secretRef || null,
    });
  }
  return (
    <form className="grid gap-4 md:grid-cols-2" onSubmit={submit}>
      <div className="flex items-center justify-between gap-4 md:col-span-2">
        <FieldLabel htmlFor="weather-enabled">
          {t("administration.weatherFeed.enabled")}
        </FieldLabel>
        <Switch
          checked={enabled}
          id="weather-enabled"
          onCheckedChange={(value) => {
            setEnabled(value);
          }}
        />
      </div>
      <Field>
        <FieldLabel htmlFor="weather-endpoint">
          {t("administration.weatherFeed.endpoint")}
        </FieldLabel>
        <Input
          id="weather-endpoint"
          onChange={(event) => {
            setEndpoint(event.target.value);
          }}
          required={enabled}
          value={endpoint}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor="weather-secret">
          {t("administration.weatherFeed.secretRef")}
        </FieldLabel>
        <Input
          id="weather-secret"
          onChange={(event) => {
            setSecretRef(event.target.value);
          }}
          value={secretRef}
        />
      </Field>
      <Button className="w-fit" disabled={save.isPending} type="submit">
        {t("administration.save")}
      </Button>
    </form>
  );
}
