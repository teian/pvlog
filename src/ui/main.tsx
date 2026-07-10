import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "@/app";
import "@/index.css";
import { loadRuntimeConfig } from "@/shared/config";
import i18n from "@/shared/lib/i18n";
import { initializeTelemetry } from "@/shared/lib/telemetry";

function requireRootElement() {
  const element = document.querySelector<HTMLDivElement>("#root");
  if (element === null) {
    throw new Error("PVLog root element is missing");
  }
  return element;
}

const rootElement = requireRootElement();

async function bootstrap() {
  const runtimeConfig = await loadRuntimeConfig();
  initializeTelemetry(runtimeConfig.telemetry);

  createRoot(rootElement).render(
    <StrictMode>
      <App runtimeConfig={runtimeConfig} />
    </StrictMode>,
  );
}

void bootstrap().catch(() => {
  rootElement.textContent = i18n.t("bootstrap.failure");
});
