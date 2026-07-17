import { useSession } from "@/features/auth";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { AppShell } from "@/widgets";
import { ShieldXIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";

/** Displays a permission-denied route state. @returns The forbidden page. */
export function ForbiddenPage() {
  const { t } = useTranslation();
  const session = useSession();
  return (
    <AppShell systemIds={session.data?.systemIds}>
      <section
        aria-labelledby="forbidden-title"
        className="flex flex-col gap-6"
      >
        <header className="flex flex-col gap-1">
          <h1
            className="text-2xl font-bold tracking-tight"
            id="forbidden-title"
          >
            {t("errors.forbiddenTitle")}
          </h1>
          <p className="text-sm text-muted-foreground">
            {t("errors.forbiddenDescription")}
          </p>
        </header>
        <Card className="max-w-2xl">
          <CardHeader>
            <div className="mb-2 flex size-10 items-center justify-center rounded-full bg-destructive/10 text-destructive">
              <ShieldXIcon aria-hidden="true" className="size-5" />
            </div>
            <CardTitle>{t("errors.forbiddenCardTitle")}</CardTitle>
            <CardDescription>
              {t("errors.forbiddenCardDescription")}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button asChild>
              <Link to="/">{t("errors.returnToDashboard")}</Link>
            </Button>
          </CardContent>
        </Card>
      </section>
    </AppShell>
  );
}
