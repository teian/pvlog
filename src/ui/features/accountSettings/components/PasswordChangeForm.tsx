import { useChangeAccountPassword } from "@/features/accountSettings/hooks/useAccountSettings";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
} from "@/shared/components";
import { SessionRequestError } from "@/shared/api/sessionRequest";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Changes a local password with client-side confirmation and server-side current-password verification. @returns Password settings card. */
// eslint-disable-next-line complexity, max-lines-per-function -- Keep transient password validation in one component so secrets never enter shared state.
export function PasswordChangeForm() {
  const { t } = useTranslation();
  const change = useChangeAccountPassword();
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const lengthInvalid = newPassword.length > 0 && newPassword.length < 12;
  const mismatch = confirmation.length > 0 && confirmation !== newPassword;
  const valid =
    currentPassword.length > 0 &&
    newPassword.length >= 12 &&
    newPassword.length <= 128 &&
    confirmation === newPassword;

  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!valid) return;
    change.mutate(
      { currentPassword, newPassword },
      {
        onSuccess: () => {
          setCurrentPassword("");
          setNewPassword("");
          setConfirmation("");
        },
      },
    );
  }

  const rejected =
    change.error instanceof SessionRequestError && change.error.status === 401;

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("account.password.title")}</CardTitle>
        <CardDescription>{t("account.password.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <form className="space-y-5" onSubmit={submit}>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="current-password">
                {t("account.password.current")}
              </FieldLabel>
              <Input
                autoComplete="current-password"
                id="current-password"
                onChange={(event) => {
                  setCurrentPassword(event.target.value);
                  change.reset();
                }}
                type="password"
                value={currentPassword}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="new-password">
                {t("account.password.new")}
              </FieldLabel>
              <Input
                aria-invalid={lengthInvalid}
                autoComplete="new-password"
                id="new-password"
                maxLength={128}
                onChange={(event) => {
                  setNewPassword(event.target.value);
                  change.reset();
                }}
                type="password"
                value={newPassword}
              />
              <FieldDescription>
                {t("account.password.requirements")}
              </FieldDescription>
              {lengthInvalid ? (
                <FieldError>{t("account.password.tooShort")}</FieldError>
              ) : null}
            </Field>
            <Field>
              <FieldLabel htmlFor="confirm-password">
                {t("account.password.confirm")}
              </FieldLabel>
              <Input
                aria-invalid={mismatch}
                autoComplete="new-password"
                id="confirm-password"
                maxLength={128}
                onChange={(event) => {
                  setConfirmation(event.target.value);
                  change.reset();
                }}
                type="password"
                value={confirmation}
              />
              {mismatch ? (
                <FieldError>{t("account.password.mismatch")}</FieldError>
              ) : null}
            </Field>
          </FieldGroup>
          {change.isSuccess ? (
            <p className="text-sm text-success" role="status">
              {t("account.password.saved")}
            </p>
          ) : null}
          {change.isError ? (
            <p className="text-sm text-destructive" role="alert">
              {rejected
                ? t("account.password.currentRejected")
                : t("account.password.saveError")}
            </p>
          ) : null}
          <Button disabled={!valid || change.isPending} type="submit">
            {change.isPending
              ? t("account.password.saving")
              : t("account.password.save")}
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}
