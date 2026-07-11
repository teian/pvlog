import { activate } from "@/features/auth";
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
import { Link, useSearchParams } from "react-router";

/** Completes invitation activation with an initial password. @returns The activation page. */
export function ActivationPage() {
  const { t } = useTranslation();
  const [params] = useSearchParams();
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [done, setDone] = useState(false);
  const token = params.get("token") ?? "";
  return (
    <main className="mx-auto flex min-h-screen max-w-md items-center px-6 py-10">
      <Card className="w-full">
        <CardHeader>
          <CardTitle aria-level={1} role="heading">
            {t("auth.activationTitle")}
          </CardTitle>
          <CardDescription>
            {done ? t("auth.activationDone") : t("auth.activationDescription")}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="activation-display-name">
                {t("auth.displayName")}
              </FieldLabel>
              <Input
                id="activation-display-name"
                onChange={(event) => {
                  setDisplayName(event.target.value);
                }}
                value={displayName}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="activation-password">
                {t("auth.newPassword")}
              </FieldLabel>
              <Input
                id="activation-password"
                onChange={(event) => {
                  setPassword(event.target.value);
                }}
                type="password"
                value={password}
              />
            </Field>
          </FieldGroup>
        </CardContent>
        <CardFooter className="justify-between">
          <Button asChild variant="link">
            <Link to="/login">{t("auth.backToLogin")}</Link>
          </Button>
          <Button
            disabled={done || !token || !displayName || !password}
            onClick={() => {
              void activate(token, displayName, password).then(() => {
                setDone(true);
              });
            }}
          >
            {t("auth.activate")}
          </Button>
        </CardFooter>
      </Card>
    </main>
  );
}
