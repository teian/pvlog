import type { SetupItem } from "@/entities/system";
import { useCallback, useState } from "react";

type ItemKey = SetupItem["key"];

const storageKey = (systemId: string) =>
  `pvlog.setup-checklist.dismissed.${systemId}`;

function read(systemId: string): ItemKey[] {
  try {
    const stored: unknown = JSON.parse(
      window.localStorage.getItem(storageKey(systemId)) ?? "[]",
    );
    return Array.isArray(stored)
      ? stored.filter((key): key is ItemKey => typeof key === "string")
      : [];
  } catch {
    return [];
  }
}

/**
 * Remembers which setup items a person chose to hide for a system.
 *
 * The choice is a personal view preference, so it stays in this browser; when
 * storage is unavailable the items simply come back after a reload.
 *
 * @param systemId - System the checklist belongs to.
 * @returns The hidden item keys and a function that hides one more.
 */
export function useDismissedSetupItems(systemId: string) {
  const [state, setState] = useState(() => ({
    systemId,
    keys: read(systemId),
  }));
  const keys = state.systemId === systemId ? state.keys : read(systemId);
  const dismiss = useCallback(
    (key: ItemKey) => {
      const next = [...new Set([...read(systemId), key])];
      try {
        window.localStorage.setItem(storageKey(systemId), JSON.stringify(next));
      } catch {
        // The item stays hidden for this visit only.
      }
      setState({ systemId, keys: next });
    },
    [systemId],
  );
  return { dismissed: keys, dismiss };
}
