import type { SourceProfile } from "@/entities/dataSource";
import { Button, Field, FieldLabel, Input } from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Asks for the profile and name of a new data source. @param props - Offered profiles, request state and the submit callback. @returns A compact form whose name defaults to the profile's name. */
export function DataSourceCreateForm({
  profiles,
  pending,
  failed,
  onSubmit,
}: {
  profiles: SourceProfile[];
  pending: boolean;
  failed: boolean;
  onSubmit: (input: { name: string; profile: string }) => void;
}) {
  const { t } = useTranslation();
  const [profile, setProfile] = useState(profiles[0]?.id ?? "");
  const [name, setName] = useState("");
  const label = (id: string, fallback: string) =>
    t(`dataSources.profiles.${id}.name`, { defaultValue: fallback });
  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!profile) return;
    const fallback =
      profiles.find((entry) => entry.id === profile)?.displayName ?? profile;
    onSubmit({ name: name.trim() || label(profile, fallback), profile });
    setName("");
  }
  return (
    <form
      className="flex flex-col gap-3 rounded-md border p-4 sm:flex-row sm:items-end"
      onSubmit={submit}
    >
      <Field className="sm:w-56">
        <FieldLabel htmlFor="data-source-profile">
          {t("dataSources.profile")}
        </FieldLabel>
        <select
          className="h-9 rounded-md border border-input bg-transparent px-3 text-sm"
          id="data-source-profile"
          onChange={(event) => {
            setProfile(event.target.value);
          }}
          value={profile}
        >
          {profiles.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {label(entry.id, entry.displayName)}
            </option>
          ))}
        </select>
      </Field>
      <Field className="flex-1">
        <FieldLabel htmlFor="data-source-name">
          {t("dataSources.name")}
        </FieldLabel>
        <Input
          id="data-source-name"
          onChange={(event) => {
            setName(event.target.value);
          }}
          value={name}
        />
      </Field>
      <Button disabled={pending || !profile} type="submit">
        {pending ? t("setup.dataSource.connecting") : t("dataSources.add")}
      </Button>
      {failed ? (
        <p className="text-sm text-destructive sm:basis-full" role="alert">
          {t("dataSources.errors.createDescription")}
        </p>
      ) : null}
    </form>
  );
}
