import { useTranslation } from "react-i18next";

/**
 * Displays the initial application placeholder while vertical slices are added.
 *
 * @returns The accessible initial page.
 */
export function HomePage() {
  const { t } = useTranslation();

  return (
    <main className="mx-auto flex min-h-screen max-w-screen-xl flex-col gap-6 px-6 py-6">
      <h1 className="text-2xl font-bold tracking-tight">{t("home.title")}</h1>
      <p className="text-sm text-muted-foreground">{t("home.description")}</p>
    </main>
  );
}
