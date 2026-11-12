import type { DataState } from "@/entities/system/system.types";
import { Badge } from "@/shared/components";
import { useTranslation } from "react-i18next";

const variants = {
  current: "success",
  stale: "warning",
  none: "outline",
} as const;

/**
 * Shows whether a system receives readings, as derived from its telemetry.
 *
 * @param props - The system's data state.
 * @returns A labelled badge; the state is never conveyed by colour alone.
 */
export function DataStateBadge({ state }: { state: DataState }) {
  const { t } = useTranslation();
  return (
    <Badge variant={variants[state]}>{t(`system.dataState.${state}`)}</Badge>
  );
}
