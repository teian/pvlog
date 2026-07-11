import { useTranslation } from "react-i18next";

/** Displays a permission-denied route state. @returns The forbidden page. */
export function ForbiddenPage() {
  const { t } = useTranslation();
  return (
    <main>
      <h1 className="text-2xl font-bold tracking-tight">
        {t("errors.forbiddenTitle")}
      </h1>
      <p className="text-sm text-muted-foreground">
        {t("errors.forbiddenDescription")}
      </p>
    </main>
  );
}
