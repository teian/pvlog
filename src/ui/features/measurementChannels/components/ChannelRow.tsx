/* eslint-disable max-lines-per-function -- one row owns the name field, the binding choice and the confirmed retire action of its channel */
import {
  ChannelTargetSelect,
  parseTarget,
  targetValue,
} from "@/features/measurementChannels/components/ChannelTargetSelect";
import type {
  BindableInverter,
  ChannelBinding,
  MeasurementChannel,
} from "@/features/measurementChannels/measurementChannels.types";
import { formatChannelValue } from "@/features/measurementChannels/utils/formatChannelValue";
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
  Input,
  TableCell,
  TableRow,
} from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Shows one measurement channel with its last value and binding. @param props - The channel, the system's equipment, whether it may only be viewed, whether a request runs, and the bind and retire callbacks. @returns A table row that highlights an unassigned channel. */
export function ChannelRow({
  channel,
  equipment,
  readOnly,
  busy,
  onBind,
  onRetire,
}: {
  channel: MeasurementChannel;
  equipment: BindableInverter[];
  readOnly: boolean;
  busy: boolean;
  onBind: (binding: ChannelBinding) => void;
  onRetire: () => void;
}) {
  const { t, i18n } = useTranslation();
  const [name, setName] = useState(channel.displayName);
  const unassigned = channel.targetKind === "unassigned";
  const bind = (next: Partial<ChannelBinding>) => {
    onBind({
      targetKind: channel.targetKind,
      targetId: channel.targetId,
      displayName: name.trim() || channel.channelKey,
      ...next,
    });
  };
  const value =
    channel.lastValue === null
      ? t("channels.noValue")
      : formatChannelValue(
          channel.lastValue,
          channel.scale,
          channel.unit,
          i18n.language,
        );
  const time =
    channel.lastValueAtEpochMillis === null
      ? null
      : new Intl.DateTimeFormat(i18n.language, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(channel.lastValueAtEpochMillis);
  return (
    <TableRow className={unassigned ? "bg-warning/10" : undefined}>
      <TableCell>
        {readOnly ? (
          <span className="font-medium">{channel.displayName}</span>
        ) : (
          <form
            className="flex gap-2"
            onSubmit={(event) => {
              event.preventDefault();
              bind({});
            }}
          >
            <Input
              aria-label={t("channels.nameOf", { key: channel.channelKey })}
              onChange={(event) => {
                setName(event.target.value);
              }}
              value={name}
            />
            {name.trim() !== channel.displayName && name.trim() !== "" ? (
              <Button disabled={busy} size="sm" type="submit">
                {t("channels.saveName")}
              </Button>
            ) : null}
          </form>
        )}
        <span className="font-mono text-xs text-muted-foreground">
          {channel.channelKey}
        </span>
      </TableCell>
      <TableCell>
        <span className="font-mono">{value}</span>
        {time ? (
          <span className="block text-xs text-muted-foreground">{time}</span>
        ) : null}
      </TableCell>
      <TableCell>
        <div className="flex flex-wrap items-center gap-2">
          <ChannelTargetSelect
            disabled={readOnly || busy}
            equipment={equipment}
            label={t("channels.targetOf", { name: channel.displayName })}
            onChange={(next) => {
              bind(parseTarget(next));
            }}
            value={targetValue(channel.targetKind, channel.targetId)}
          />
          {unassigned ? (
            <Badge variant="warning">{t("channels.unassignedBadge")}</Badge>
          ) : null}
        </div>
      </TableCell>
      {readOnly ? null : (
        <TableCell>
          <AlertDialog>
            <AlertDialogTrigger asChild>
              <Button disabled={busy} size="sm" variant="outline">
                {t("channels.retire.action")}
              </Button>
            </AlertDialogTrigger>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>
                  {t("channels.retire.title")}
                </AlertDialogTitle>
                <AlertDialogDescription>
                  {t("channels.retire.description", {
                    name: channel.displayName,
                  })}
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>{t("dataSources.cancel")}</AlertDialogCancel>
                <AlertDialogAction onClick={onRetire}>
                  {t("channels.retire.confirm")}
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </TableCell>
      )}
    </TableRow>
  );
}
