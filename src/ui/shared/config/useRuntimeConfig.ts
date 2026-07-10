import { use } from "react";

import { RuntimeConfigContext } from "./RuntimeConfigContext";

/**
 * Reads validated deployment configuration.
 *
 * @returns Runtime configuration for the current deployment.
 */
export function useRuntimeConfig() {
  const value = use(RuntimeConfigContext);

  if (value === undefined) {
    throw new Error(
      "Runtime configuration is unavailable outside its provider",
    );
  }

  return value;
}
