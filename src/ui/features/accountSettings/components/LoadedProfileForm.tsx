import { useUpdateAccountProfile } from "@/features/accountSettings/hooks/useAccountSettings";
import type { AccountProfile } from "@/features/accountSettings/types/accountSettings.types";
import {
  Button,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
} from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Renders editable fields after profile loading has completed. @param props - Loaded current-user profile. @returns Profile form. */
export function LoadedProfileForm({ profile }: { profile: AccountProfile }) {
  const { t } = useTranslation();
  const update = useUpdateAccountProfile();
  const [displayName, setDisplayName] = useState(profile.displayName);
  const normalizedName = displayName.trim();
  const nameInvalid =
    normalizedName.length === 0 || normalizedName.length > 120;

  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    if (nameInvalid) return;
    update.mutate({ displayName: normalizedName });
  }

  return (
    <form className="space-y-5" onSubmit={submit}>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="account-display-name">
            {t("account.profile.displayName")}
          </FieldLabel>
          <Input
            aria-invalid={nameInvalid}
            autoComplete="name"
            id="account-display-name"
            maxLength={120}
            onChange={(event) => {
              setDisplayName(event.target.value);
              update.reset();
            }}
            value={displayName}
          />
          {nameInvalid ? (
            <FieldError>{t("account.profile.nameInvalid")}</FieldError>
          ) : null}
        </Field>
        <Field>
          <FieldLabel htmlFor="account-email">
            {t("account.profile.email")}
          </FieldLabel>
          <Input
            aria-describedby="account-email-description"
            autoComplete="email"
            disabled
            id="account-email"
            type="email"
            value={profile.email}
          />
          <FieldDescription id="account-email-description">
            {t("account.profile.emailReadOnly")}
          </FieldDescription>
        </Field>
      </FieldGroup>
      {update.isSuccess ? (
        <p className="text-sm text-success" role="status">
          {t("account.profile.saved")}
        </p>
      ) : null}
      {update.isError ? (
        <p className="text-sm text-destructive" role="alert">
          {t("account.profile.saveError")}
        </p>
      ) : null}
      <Button
        disabled={
          nameInvalid ||
          update.isPending ||
          normalizedName === profile.displayName
        }
        type="submit"
      >
        {update.isPending
          ? t("account.profile.saving")
          : t("account.profile.save")}
      </Button>
    </form>
  );
}
