import { createContext } from "react";

import type { RuntimeConfig } from "./runtimeConfig";

/** Internal context for validated deployment configuration. */
export const RuntimeConfigContext = createContext<RuntimeConfig | undefined>(
  undefined,
);
