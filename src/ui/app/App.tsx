import { AppProviders } from "@/app/AppProviders";
import { HomePage } from "@/pages";
import type { RuntimeConfig } from "@/shared/config";
import { lazy, Suspense } from "react";
import { Route, Routes } from "react-router";

const ApiReferencePage = lazy(async () => {
  const page = await import("@/pages/ApiReferencePage");
  return { default: page.ApiReferencePage };
});

/** Application root properties. */
export interface AppProps {
  /** Validated deployment configuration. */
  runtimeConfig: RuntimeConfig;
}

/**
 * Composes global providers and the application router.
 *
 * @param props - Application root properties.
 * @returns The PVLog application tree.
 */
export function App({ runtimeConfig }: AppProps) {
  return (
    <AppProviders runtimeConfig={runtimeConfig}>
      <Suspense fallback={null}>
        <Routes>
          <Route element={<ApiReferencePage />} path="/docs/api" />
          <Route element={<HomePage />} path="*" />
        </Routes>
      </Suspense>
    </AppProviders>
  );
}
