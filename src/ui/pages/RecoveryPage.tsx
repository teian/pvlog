import { requestRecovery } from "@/features/auth";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
  Field,
  FieldGroup,
  FieldLabel,
  Input,
} from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";

/** Starts enumeration-safe local account recovery. @returns The recovery page. */
export function RecoveryPage() {
  const { t } = useTranslation();
  const [email, setEmail] = useState("");
  const [sent, setSent] = useState(false);
  return (
    <main className="mx-auto flex min-h-screen max-w-md items-center px-6 py-10">
      <Card className="w-full">
        <CardHeader>
          <CardTitle aria-level={1} role="heading">
            {t("auth.recoveryTitle")}
          </CardTitle>
          <CardDescription>
            {sent ? t("auth.recoverySent") : t("auth.recoveryDescription")}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="recovery-email">
                {t("auth.email")}
              </FieldLabel>
              <Input
                id="recovery-email"
                onChange={(event) => {
                  setEmail(event.target.value);
                }}
                type="email"
                value={email}
              />
            </Field>
          </FieldGroup>
        </CardContent>
        <CardFooter className="justify-between">
          <Button asChild variant="link">
            <Link to="/login">{t("auth.backToLogin")}</Link>
          </Button>
          <Button
            disabled={sent}
            onClick={() => {
              void requestRecovery(email).then(() => {
                setSent(true);
              });
            }}
          >
            {t("auth.sendRecovery")}
          </Button>
        </CardFooter>
      </Card>
    </main>
  );
}
