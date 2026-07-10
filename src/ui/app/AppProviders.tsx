import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { PropsWithChildren } from "react";
import { useState } from "react";
import { BrowserRouter } from "react-router";

import { RuntimeConfigProvider, type RuntimeConfig } from "@/shared/config";

/** Application provider properties. */
export interface AppProvidersProps extends PropsWithChildren {
  /** Validated deployment configuration. */
  runtimeConfig: RuntimeConfig;
}

/**
 * Installs process-wide UI providers.
 *
 * @param props - Provider children.
 * @returns The provider composition.
 */
export function AppProviders({ children, runtimeConfig }: AppProvidersProps) {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            retry: 1,
            staleTime: 0,
          },
        },
      }),
  );

  return (
    <RuntimeConfigProvider value={runtimeConfig}>
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>{children}</BrowserRouter>
      </QueryClientProvider>
    </RuntimeConfigProvider>
  );
}
