import type { PropsWithChildren } from "react";

import { RuntimeConfigContext } from "./RuntimeConfigContext";
import type { RuntimeConfig } from "./runtimeConfig";

/** Runtime configuration provider properties. */
export interface RuntimeConfigProviderProps extends PropsWithChildren {
  /** Validated deployment configuration. */
  value: RuntimeConfig;
}

/**
 * Makes validated deployment configuration available to UI features.
 *
 * @param props - Provider properties and children.
 * @returns The runtime configuration provider.
 */
export function RuntimeConfigProvider({
  children,
  value,
}: RuntimeConfigProviderProps) {
  return <RuntimeConfigContext value={value}>{children}</RuntimeConfigContext>;
}
