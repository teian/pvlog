import { ApiReferenceReact } from "@scalar/api-reference-react";
import { useTranslation } from "react-i18next";

/**
 * Renders the locally packaged API reference for the committed contract.
 *
 * @returns The interactive API documentation page.
 */
export function ApiReferencePage() {
  const { t } = useTranslation();

  return (
    <main aria-label={t("docs.apiReferenceLabel")}>
      <nav
        aria-label={t("docs.apiVersionNavigation")}
        className="flex items-center justify-between border-b px-4 py-2"
      >
        <label
          className="flex items-center gap-2 text-sm"
          htmlFor="api-version"
        >
          {t("docs.apiVersion")}
          <select
            className="rounded border bg-background px-2 py-1"
            id="api-version"
          >
            <option value="1.0">{"1.0"}</option>
          </select>
        </label>
        <a className="text-sm underline" download href="/openapi/pvlog-v1.yaml">
          {t("docs.downloadOpenApi")}
        </a>
      </nav>
      <ApiReferenceReact
        configuration={{
          url: "/openapi/pvlog-v1.yaml",
          theme: "none",
          layout: "modern",
          hideModels: false,
        }}
      />
    </main>
  );
}
