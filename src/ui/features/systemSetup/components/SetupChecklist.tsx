import { useSetupStatus, type SetupItem } from "@/entities/system";
import { useDismissedSetupItems } from "@/features/systemSetup/hooks/useDismissedSetupItems";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { XIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";

/** Where each setup item is completed, relative to the system page. */
const DESTINATIONS: Record<SetupItem["key"], string> = {
  location: "settings/general",
  capacity: "settings/general",
  data_source: "settings/data-sources",
  first_data: "settings/data-sources",
  channels: "settings/channels",
  equipment: "settings/equipment",
  tariff: "settings/tariffs",
  forecast: "forecast",
};

/**
 * Lists what a system still needs, each item linking to where it is done.
 *
 * Nothing renders once every item is done or hidden, so a finished system is
 * not followed around by its setup.
 *
 * @param props - The system whose setup status is shown.
 * @returns The open setup items, or nothing.
 */
export function SetupChecklist({ systemId }: { systemId: string }) {
  const { t } = useTranslation();
  const status = useSetupStatus(systemId);
  const { dismissed, dismiss } = useDismissedSetupItems(systemId);
  const open = (status.data?.items ?? []).filter(
    (item) => item.state !== "done" && !dismissed.includes(item.key),
  );
  if (open.length === 0) return null;
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          <h2 id="setup-checklist-title">{t("setup.checklist.title")}</h2>
        </CardTitle>
        <CardDescription>{t("setup.checklist.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <ul
          aria-labelledby="setup-checklist-title"
          className="flex flex-col divide-y"
        >
          {open.map((item) => {
            const title = t(`setup.checklist.items.${item.key}`);
            return (
              <li
                className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2"
                key={item.key}
              >
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="flex flex-wrap items-center gap-2 text-sm font-semibold">
                    {title}
                    {item.state === "recommended" ? (
                      <Badge>{t("setup.checklist.next")}</Badge>
                    ) : null}
                  </span>
                  {item.reason ? (
                    <span className="text-sm text-muted-foreground">
                      {t(`setup.checklist.reasons.${item.reason}`, {
                        count: item.count ?? 0,
                        defaultValue: "",
                      })}
                    </span>
                  ) : null}
                </span>
                <Button asChild size="sm" variant="outline">
                  <Link
                    aria-label={t("setup.checklist.openLabel", { item: title })}
                    to={`/systems/${systemId}/${DESTINATIONS[item.key]}`}
                  >
                    {t("setup.checklist.open")}
                  </Link>
                </Button>
                <Button
                  aria-label={t("setup.checklist.hideLabel", { item: title })}
                  onClick={() => {
                    dismiss(item.key);
                  }}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <XIcon aria-hidden="true" />
                </Button>
              </li>
            );
          })}
        </ul>
      </CardContent>
    </Card>
  );
}
