import { useInviteUser } from "@/features/administration/hooks/useAdministration";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Field,
  FieldLabel,
  Input,
} from "@/shared/components";
import { Plus } from "lucide-react";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Opens the real local-user invitation flow from the compact user table. @returns An accessible invitation dialog and trigger. */
export function InviteUserDialog() {
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
    <Dialog>
      <DialogTrigger asChild>
        <Button size="sm">
          <Plus aria-hidden="true" />
          {t("administration.users.invite")}
        </Button>
      </DialogTrigger>
      <DialogContent closeLabel={t("administration.users.closeInvite")}>
        <DialogHeader>
          <DialogTitle>{t("administration.invitations.title")}</DialogTitle>
          <DialogDescription>
            {t("administration.invitations.description")}
          </DialogDescription>
        </DialogHeader>
        <form className="space-y-4" onSubmit={submit}>
          <Field>
            <FieldLabel htmlFor="invitation-email-dialog">
              {t("administration.invitations.email")}
            </FieldLabel>
            <Input
              id="invitation-email-dialog"
              onChange={(event) => {
                setEmail(event.target.value);
              }}
              required
              type="email"
              value={email}
            />
          </Field>
          {inviteUser.isError ? (
            <p className="text-sm text-destructive" role="alert">
              {t("administration.invitations.error")}
            </p>
          ) : null}
          <Button disabled={inviteUser.isPending || !email} type="submit">
            {t("administration.invitations.create")}
          </Button>
        </form>
        {inviteUser.data ? (
          <Alert>
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
      </DialogContent>
    </Dialog>
  );
}
