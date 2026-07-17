/* eslint-disable max-lines-per-function -- the expandable system card owns one semantic hierarchy and its destructive confirmation */
import type { ManagedSystem } from "@/features/systemManagement/types/systemManagement.types";
import { orientationKey } from "@/features/systemManagement/utils/systemManagementDraft";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import {
  ChevronRightIcon,
  CpuIcon,
  MapPinIcon,
  PencilIcon,
  SolarPanelIcon,
  Trash2Icon,
} from "lucide-react";
import { useTranslation } from "react-i18next";

/** Displays one expandable system with nested inverter and string summaries. @param props - System, expansion state and management callbacks. @returns High-fidelity system card. */
export function SystemManagementCard({
  system,
  expanded,
  canDelete,
  onToggle,
  onEdit,
  onDelete,
}: {
  system: ManagedSystem;
  expanded: boolean;
  canDelete: boolean;
  onToggle: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const { t } = useTranslation();
  const totalWatts = system.inverters.reduce(
    (sum, inverter) =>
      sum +
      inverter.strings.reduce(
        (inner, string) => inner + string.ratedPowerWatts,
        0,
      ),
    0,
  );
  const stringCount = system.inverters.reduce(
    (sum, inverter) => sum + inverter.strings.length,
    0,
  );
  const active = system.record.lifecycle === "active";
  return (
    <Card className="gap-0 overflow-hidden py-0 shadow-sm">
      <CardHeader className="grid grid-cols-[auto_1fr_auto] items-center gap-3 border-b px-[18px] py-[15px] sm:grid-cols-[auto_1fr_auto_auto_auto]">
        <Button
          aria-expanded={expanded}
          aria-label={t("systemManagement.actions.toggle", {
            name: system.record.name,
          })}
          onClick={onToggle}
          size="icon-xs"
          variant="ghost"
        >
          <ChevronRightIcon
            className={
              expanded
                ? "rotate-90 transition-transform motion-reduce:transition-none"
                : "transition-transform motion-reduce:transition-none"
            }
          />
        </Button>
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <CardTitle>
              <h2 className="truncate text-[15px] font-extrabold tracking-[-0.01em]">
                {system.record.name}
              </h2>
            </CardTitle>
            <Badge
              className="font-mono text-[9px] tracking-[0.08em]"
              variant={active ? "success" : "secondary"}
            >
              {t(
                active
                  ? "systemManagement.status.activeUpper"
                  : "systemManagement.status.inactiveUpper",
              )}
            </Badge>
          </div>
          <p className="mt-1 flex items-center gap-1 text-xs text-muted-foreground">
            <MapPinIcon aria-hidden="true" className="size-3" />
            {t("systemManagement.noLocation")}
          </p>
        </div>
        <div className="text-right">
          <p className="font-mono text-[15px] font-semibold text-brand-foreground">
            {(totalWatts / 1000).toFixed(1)} {t("systemManagement.units.kwp")}
          </p>
          <p className="text-[10px] text-muted-foreground">
            {t("systemManagement.summary", {
              inverters: system.inverters.length,
              strings: stringCount,
            })}
          </p>
        </div>
        <Button
          className="hidden text-primary sm:inline-flex"
          onClick={onEdit}
          size="sm"
          variant="outline"
        >
          <PencilIcon data-icon="inline-start" />
          {t("systemManagement.actions.edit")}
        </Button>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button
              aria-label={t("systemManagement.actions.deleteSystem", {
                name: system.record.name,
              })}
              disabled={!canDelete}
              size="icon-sm"
              variant="destructive"
            >
              <Trash2Icon />
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>
                {t("systemManagement.delete.title")}
              </AlertDialogTitle>
              <AlertDialogDescription>
                {t("systemManagement.delete.description", {
                  name: system.record.name,
                })}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>
                {t("systemManagement.actions.cancel")}
              </AlertDialogCancel>
              <AlertDialogAction onClick={onDelete} variant="destructive">
                {t("systemManagement.actions.delete")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
        <Button
          className="col-span-3 sm:hidden"
          onClick={onEdit}
          size="sm"
          variant="outline"
        >
          <PencilIcon data-icon="inline-start" />
          {t("systemManagement.actions.edit")}
        </Button>
      </CardHeader>
      {expanded ? (
        <CardContent className="flex flex-col gap-4 px-5 py-[18px]">
          {system.inverters.map((inverter, index) => {
            const inverterWatts = inverter.strings.reduce(
              (sum, string) => sum + string.ratedPowerWatts,
              0,
            );
            return (
              <section className="flex flex-col gap-2" key={inverter.id}>
                <header className="flex items-center gap-2">
                  <span className="flex size-[22px] items-center justify-center rounded-sm bg-primary font-mono text-xs font-bold text-primary-foreground">
                    {index + 1}
                  </span>
                  <CpuIcon
                    aria-hidden="true"
                    className="size-[15px] text-primary"
                  />
                  <h3 className="truncate text-[13px] font-bold">
                    {inverter.model ?? inverter.name}
                  </h3>
                  {inverter.ratedPowerWatts ? (
                    <Badge
                      className="font-mono text-[10px]"
                      variant="secondary"
                    >
                      {(inverter.ratedPowerWatts / 1000).toFixed(1)}{" "}
                      {t("systemManagement.units.kwMax")}
                    </Badge>
                  ) : null}
                  <span className="ml-auto font-mono text-xs font-semibold text-muted-foreground">
                    {t("systemManagement.inverterSummary", {
                      count: inverter.strings.length,
                      kwp: (inverterWatts / 1000).toFixed(1),
                    })}
                  </span>
                </header>
                <div className="ml-[10px] flex flex-col gap-2 border-l-2 pl-[10px]">
                  {inverter.strings.length === 0 ? (
                    <p className="text-[11px] italic text-muted-foreground">
                      {t("systemManagement.emptyInverter")}
                    </p>
                  ) : (
                    inverter.strings.map((string) => (
                      <article
                        className="flex items-center gap-3 rounded-md border bg-card p-3 shadow-xs"
                        key={string.id}
                      >
                        <span className="flex size-[34px] items-center justify-center rounded-md border border-brand/20 bg-brand/5 text-brand">
                          <SolarPanelIcon aria-hidden="true" />
                        </span>
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-[13px] font-semibold">
                            {string.name}
                          </p>
                          <p className="truncate text-[11px] text-muted-foreground">
                            {t("systemManagement.stringDescription", {
                              count: string.panelCount,
                              watts:
                                string.modulePeakPowerWatts ??
                                Math.round(
                                  string.ratedPowerWatts / string.panelCount,
                                ),
                              orientation: t(
                                `systemManagement.orientation.${orientationKey(string.orientationDegrees)}`,
                              ),
                            })}
                          </p>
                        </div>
                        <span className="font-mono text-[13px] font-semibold text-brand-foreground">
                          {(string.ratedPowerWatts / 1000).toFixed(2)}{" "}
                          {t("systemManagement.units.kwp")}
                        </span>
                        <Badge
                          className="font-mono text-[9px] tracking-[0.06em]"
                          variant={active ? "success" : "secondary"}
                        >
                          {t(
                            active
                              ? "systemManagement.status.online"
                              : "systemManagement.status.offline",
                          )}
                        </Badge>
                      </article>
                    ))
                  )}
                </div>
              </section>
            );
          })}
        </CardContent>
      ) : null}
    </Card>
  );
}
