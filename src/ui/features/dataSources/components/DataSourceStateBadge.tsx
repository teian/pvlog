import type { ConnectionState } from "@/entities/dataSource";
import { Badge } from "@/shared/components";
import { useTranslation } from "react-i18next";

const variants = {
  never_connected: "outline",
  receiving: "success",
  stale: "warning",
  disconnected: "fault",
} as const;

/** Shows whether a data source still delivers readings. @param props - Derived connection state. @returns A labelled badge; the state is never conveyed by colour alone. */
export function DataSourceStateBadge({ state }: { state: ConnectionState }) {
  const { t } = useTranslation();
  return (
    <Badge variant={variants[state]}>{t(`dataSources.states.${state}`)}</Badge>
  );
}
