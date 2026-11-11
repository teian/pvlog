/* eslint-disable max-lines-per-function -- profile choice, name and the one-time result belong to one step */
import {
  DataSourceConfigurationView,
  useCreateDataSource,
  useSourceProfiles,
  type DataSource,
  type DataSourceConfiguration,
} from "@/entities/dataSource";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Field,
  FieldLabel,
  FieldLegend,
  FieldSet,
  Input,
  Skeleton,
} from "@/shared/components";
import { cn } from "@/shared/lib/utils";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Second setup step: connect the uploader that feeds the system. @param props - The system to feed and callbacks for a connected source or a skipped step. @returns A profile choice that, once created, shows the configuration exactly once. */
export function SetupDataSourceStep({
  systemId,
  onConnected,
  onSkip,
}: {
  systemId: string;
  onConnected: (dataSource: DataSource) => void;
  onSkip: () => void;
}) {
  const { t } = useTranslation();
  const profiles = useSourceProfiles();
  const [profile, setProfile] = useState<string>();
  const [name, setName] = useState("");
  const [issued, setIssued] = useState<{
    dataSource: DataSource;
    configuration: DataSourceConfiguration | null;
  }>();
  const create = useCreateDataSource(systemId, (dataSource, configuration) => {
    setIssued({ dataSource, configuration });
  });
  const selected = profile ?? profiles.data?.[0]?.id;

  if (issued)
    return (
      <div className="flex flex-col gap-5">
        {issued.configuration ? (
          <DataSourceConfigurationView configuration={issued.configuration} />
        ) : (
          <Alert>
            <AlertTitle>{t("setup.dataSource.noKeyTitle")}</AlertTitle>
            <AlertDescription>
              {t("setup.dataSource.noKeyDescription")}
            </AlertDescription>
          </Alert>
        )}
        <Button
          className="self-start"
          onClick={() => {
            onConnected(issued.dataSource);
          }}
          type="button"
        >
          {t("setup.dataSource.continue")}
        </Button>
      </div>
    );

  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selected) return;
    create.mutate({
      name: name.trim() || t(`dataSources.profiles.${selected}.name`),
      profile: selected,
    });
  }

  if (profiles.isPending) return <Skeleton className="h-40 w-full" />;
  if (profiles.isError)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("dataSources.errors.loadTitle")}</AlertTitle>
        <AlertDescription>
          {t("dataSources.errors.loadDescription")}
        </AlertDescription>
      </Alert>
    );

  return (
    <form className="flex flex-col gap-6" onSubmit={submit}>
      <FieldSet>
        <FieldLegend>{t("setup.dataSource.profileLegend")}</FieldLegend>
        <div className="grid gap-3 sm:grid-cols-2">
          {profiles.data.map((entry) => (
            <label
              className={cn(
                "flex cursor-pointer flex-col gap-1 rounded-md border p-4 text-sm has-focus-visible:ring-2 has-focus-visible:ring-ring",
                selected === entry.id && "border-primary bg-primary/5",
              )}
              key={entry.id}
            >
              <span className="flex items-center gap-2 font-semibold">
                <input
                  checked={selected === entry.id}
                  name="setup-profile"
                  onChange={() => {
                    setProfile(entry.id);
                  }}
                  type="radio"
                  value={entry.id}
                />
                {t(`dataSources.profiles.${entry.id}.name`, {
                  defaultValue: entry.displayName,
                })}
              </span>
              <span className="text-muted-foreground">
                {t(`dataSources.profiles.${entry.id}.description`, {
                  defaultValue: "",
                })}
              </span>
            </label>
          ))}
        </div>
      </FieldSet>
      <Field>
        <FieldLabel htmlFor="setup-source-name">
          {t("dataSources.name")}
        </FieldLabel>
        <Input
          id="setup-source-name"
          onChange={(event) => {
            setName(event.target.value);
          }}
          placeholder={
            selected ? t(`dataSources.profiles.${selected}.name`) : undefined
          }
          value={name}
        />
      </Field>
      {create.isError ? (
        <p className="text-sm text-destructive" role="alert">
          {t("dataSources.errors.createDescription")}
        </p>
      ) : null}
      <div className="flex flex-wrap gap-3">
        <Button disabled={create.isPending || !selected} type="submit">
          {create.isPending
            ? t("setup.dataSource.connecting")
            : t("setup.dataSource.connect")}
        </Button>
        <Button onClick={onSkip} type="button" variant="outline">
          {t("setup.dataSource.later")}
        </Button>
      </div>
    </form>
  );
}
