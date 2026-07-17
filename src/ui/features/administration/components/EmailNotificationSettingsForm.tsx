import { useSaveEmailNotificationSettings } from "@/features/administration/hooks/useAdministration";
import type { EmailNotificationSettings } from "@/features/administration/types/administration.types";
import { Button, Field, FieldLabel, Input, Switch } from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Edits SMTP metadata while retaining credential material in an external secret store. */
export function EmailNotificationSettingsForm({
  initial,
}: {
  initial: EmailNotificationSettings;
}) {
  const { t } = useTranslation();
  const save = useSaveEmailNotificationSettings();
  const [form, setForm] = useState({
    ...initial,
    credentialSecretRef: initial.credentialSecretRef ?? "",
  });
  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    save.mutate({
      ...form,
      credentialSecretRef: form.credentialSecretRef || null,
    });
  }
  return (
    <form className="grid gap-4 md:grid-cols-2" onSubmit={submit}>
      <div className="flex items-center justify-between gap-4 md:col-span-2">
        <FieldLabel htmlFor="email-enabled">
          {t("administration.email.enabled")}
        </FieldLabel>
        <Switch
          checked={form.enabled}
          id="email-enabled"
          onCheckedChange={(enabled) => {
            setForm({ ...form, enabled });
          }}
        />
      </div>
      {(["recipient", "host", "username", "credentialSecretRef"] as const).map(
        (field) => (
          <Field key={field}>
            <FieldLabel htmlFor={`email-${field}`}>
              {t(`administration.email.${field}`)}
            </FieldLabel>
            <Input
              id={`email-${field}`}
              onChange={(event) => {
                setForm({ ...form, [field]: event.target.value });
              }}
              required={form.enabled && field !== "credentialSecretRef"}
              value={form[field]}
            />
          </Field>
        ),
      )}
      <Field>
        <FieldLabel htmlFor="email-port">
          {t("administration.email.port")}
        </FieldLabel>
        <Input
          id="email-port"
          max={65535}
          min={1}
          onChange={(event) => {
            setForm({ ...form, port: Number(event.target.value) });
          }}
          type="number"
          value={form.port}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor="email-encryption">
          {t("administration.email.encryption")}
        </FieldLabel>
        <select
          className="h-9 rounded-md border bg-background px-3 text-sm"
          id="email-encryption"
          onChange={(event) => {
            setForm({
              ...form,
              encryption: event.target.value as typeof form.encryption,
            });
          }}
          value={form.encryption}
        >
          <option value="starttls">{t("administration.email.starttls")}</option>
          <option value="tls">{t("administration.email.tls")}</option>
          <option value="none">{t("administration.email.none")}</option>
        </select>
      </Field>
      <Button className="w-fit" disabled={save.isPending} type="submit">
        {t("administration.save")}
      </Button>
    </form>
  );
}
