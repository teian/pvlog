import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
} from "@/shared/components";
import { useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router";

/** Displays normalized external connector callback states. @returns The callback status page. */
export function AuthCallbackPage() {
  const { t } = useTranslation();
  const [params] = useSearchParams();
  const state = params.get("status") ?? "processing";
  return (
    <main className="mx-auto flex min-h-screen max-w-xl items-center px-6 py-10">
      <Alert variant={state === "error" ? "destructive" : "default"}>
        <AlertTitle>{t(`auth.callback.${state}.title`)}</AlertTitle>
        <AlertDescription>
          {t(`auth.callback.${state}.description`)}
        </AlertDescription>
        {state !== "processing" ? (
          <Button asChild className="mt-4" variant="outline">
            <Link to={state === "success" ? "/" : "/login"}>
              {t("auth.callback.continue")}
            </Link>
          </Button>
        ) : null}
      </Alert>
    </main>
  );
}
