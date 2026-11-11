import { useCallback } from "react";
import { useSearchParams } from "react-router";

/** Search parameter that carries the reporting system selection. */
export const SYSTEM_SEARCH_PARAMETER = "system";

export interface SelectedSystem {
  /** The system the reporting views should render, empty when none is visible. */
  systemId: string;
  /** Replaces the current selection, keeping every other search parameter. */
  selectSystem: (systemId: string) => void;
}

/**
 * Resolves the reporting system selection from the URL.
 *
 * The selection lives in the query string so a view stays shareable and
 * survives a reload. An unknown or missing identifier falls back to the first
 * visible system instead of rendering an empty report.
 *
 * @param systemIds - Systems visible to the current session, in session order.
 * @returns The effective system identifier and a setter for the selection.
 */
export function useSelectedSystem(systemIds: string[]): SelectedSystem {
  const [searchParameters, setSearchParameters] = useSearchParams();
  const requested = searchParameters.get(SYSTEM_SEARCH_PARAMETER);
  const systemId =
    requested && systemIds.includes(requested)
      ? requested
      : (systemIds[0] ?? "");

  const selectSystem = useCallback(
    (nextSystemId: string) => {
      setSearchParameters(
        (current) => {
          const next = new URLSearchParams(current);
          next.set(SYSTEM_SEARCH_PARAMETER, nextSystemId);
          return next;
        },
        { replace: true },
      );
    },
    [setSearchParameters],
  );

  return { systemId, selectSystem };
}
