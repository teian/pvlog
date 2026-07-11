import { AppProviders } from "@/app/AppProviders";
import { AppRoutes } from "@/app/AppRoutes";
import type { RuntimeConfig } from "@/shared/config";

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
      <AppRoutes />
    </AppProviders>
  );
}
