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
            disabled={done || !token}
            onClick={() => {
              void activate(token, password).then(() => {
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
