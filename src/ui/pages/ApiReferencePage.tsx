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
