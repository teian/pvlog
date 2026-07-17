import {
  useRunBackup,
  useSaveRetentionBackupSettings,
} from "@/features/administration/hooks/useAdministration";
import type { RetentionBackupSettings } from "@/features/administration/types/administration.types";
import { Button, Field, FieldLabel, Input, Switch } from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Edits retention settings and starts an immediate operator backup. */
export function RetentionBackupSettingsForm({
  initial,
}: {
  initial: RetentionBackupSettings;
}) {
  const { t } = useTranslation();
  const save = useSaveRetentionBackupSettings();
  const backup = useRunBackup();
  const [days, setDays] = useState(initial.readingRetentionDays);
  const [automatic, setAutomatic] = useState(initial.automaticBackupsEnabled);
  const [schedule, setSchedule] = useState(initial.backupSchedule);
  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    save.mutate({
      ...initial,
      readingRetentionDays: days,
      automaticBackupsEnabled: automatic,
      backupSchedule: schedule,
    });
  }
  return (
    <form className="grid gap-4 md:grid-cols-2" onSubmit={submit}>
      <Field>
        <FieldLabel htmlFor="retention-days">
          {t("administration.retention.days")}
        </FieldLabel>
        <Input
          id="retention-days"
          max={3650}
          min={1}
          onChange={(event) => {
            setDays(Number(event.target.value));
          }}
          type="number"
          value={days}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor="backup-schedule">
          {t("administration.retention.schedule")}
        </FieldLabel>
        <Input
          id="backup-schedule"
          onChange={(event) => {
            setSchedule(event.target.value);
          }}
          value={schedule}
        />
      </Field>
      <div className="flex items-center justify-between gap-4 md:col-span-2">
        <FieldLabel htmlFor="automatic-backups">
          {t("administration.retention.automatic")}
        </FieldLabel>
        <Switch
          checked={automatic}
          id="automatic-backups"
          onCheckedChange={(value) => {
            setAutomatic(value);
          }}
        />
      </div>
      <div className="flex flex-wrap gap-3 md:col-span-2">
        <Button disabled={save.isPending} type="submit">
          {t("administration.save")}
        </Button>
        <Button
          disabled={backup.isPending}
          onClick={() => {
            backup.mutate();
          }}
          type="button"
          variant="outline"
        >
          {t("administration.retention.runBackup")}
        </Button>
      </div>
    </form>
  );
}
