import { useInviteUser } from "@/features/administration/hooks/useAdministration";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Field,
  FieldLabel,
  Input,
} from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Creates one-time invitations for local users when the browser session has instance-admin permission. @returns The invitation panel. */
export function InvitationPanel() {
  const { t } = useTranslation();
  const [email, setEmail] = useState("");
  const inviteUser = useInviteUser();

  function submit(event: SyntheticEvent<HTMLFormElement>): void {
    event.preventDefault();
    inviteUser.mutate(email, {
      onSuccess: () => {
        setEmail("");
      },
    });
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("administration.invitations.title")}</CardTitle>
        <CardDescription>
          {t("administration.invitations.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form className="space-y-4" onSubmit={submit}>
          <Field>
            <FieldLabel htmlFor="invitation-email">
              {t("administration.invitations.email")}
            </FieldLabel>
            <Input
              id="invitation-email"
              onChange={(event) => {
                setEmail(event.target.value);
              }}
              required
              type="email"
              value={email}
            />
          </Field>
          {inviteUser.isError ? (
            <p className="text-sm text-destructive">
              {t("administration.invitations.error")}
            </p>
          ) : null}
          <Button disabled={inviteUser.isPending || !email} type="submit">
            {t("administration.invitations.create")}
          </Button>
        </form>
        {inviteUser.data ? (
          <Alert className="mt-6">
            <AlertTitle>
              {t("administration.invitations.tokenTitle")}
            </AlertTitle>
            <AlertDescription>
              <p>{t("administration.invitations.tokenDescription")}</p>
              <code className="mt-2 block break-all rounded bg-muted p-2">
                {inviteUser.data.activationToken}
              </code>
            </AlertDescription>
          </Alert>
        ) : null}
      </CardContent>
    </Card>
  );
}
