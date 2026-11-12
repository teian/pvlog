import { useSourceProfiles } from "@/entities/dataSource";
import { DiscoveryProposals } from "@/entities/equipmentDiscovery";
import { ChannelRow } from "@/features/measurementChannels/components/ChannelRow";
import {
  useBindChannel,
  useBindableEquipment,
  useChannels,
  useRetireChannel,
} from "@/features/measurementChannels/useMeasurementChannels";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Skeleton,
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Lists a system's measurement channels and lets them be bound, renamed or retired; devices the channels reveal are offered for adoption first. @param props - The system and whether its channels may only be viewed. @returns The measurement channels section of the system settings. */
export function ChannelManager({
  systemId,
  readOnly,
}: {
  systemId: string;
  readOnly: boolean;
}) {
  const { t } = useTranslation();
  const channels = useChannels(systemId);
  const equipment = useBindableEquipment(systemId);
  const bind = useBindChannel(systemId);
  const retire = useRetireChannel(systemId);
  // Labels of status codes are a nicety: without the profiles codes show as numbers.
  const profiles = useSourceProfiles();

  if (channels.isPending || equipment.isPending)
    return <Skeleton className="h-40 w-full" />;
  if (channels.isError || equipment.isError)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("channels.errors.loadTitle")}</AlertTitle>
        <AlertDescription>
          {t("channels.errors.loadDescription")}
        </AlertDescription>
      </Alert>
    );
  if (channels.data.length === 0)
    return (
      <Alert>
        <AlertTitle>{t("channels.empty.title")}</AlertTitle>
        <AlertDescription>{t("channels.empty.description")}</AlertDescription>
      </Alert>
    );

  const unassigned = channels.data.filter(
    (channel) => channel.targetKind === "unassigned",
  ).length;
  // Unassigned channels come first: they are what needs attention.
  const ordered = [...channels.data].sort(
    (left, right) =>
      Number(right.targetKind === "unassigned") -
        Number(left.targetKind === "unassigned") ||
      left.displayName.localeCompare(right.displayName),
  );
  return (
    <div className="flex flex-col gap-4">
      <DiscoveryProposals readOnly={readOnly} systemId={systemId} />
      {unassigned > 0 ? (
        <Alert>
          <AlertTitle>
            {t("channels.unassigned.title", { count: unassigned })}
          </AlertTitle>
          <AlertDescription>
            {t("channels.unassigned.description")}
          </AlertDescription>
        </Alert>
      ) : null}
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("channels.columns.channel")}</TableHead>
            <TableHead>{t("channels.columns.lastValue")}</TableHead>
            <TableHead>{t("channels.columns.target")}</TableHead>
            {readOnly ? null : (
              <TableHead>{t("dataSources.columns.actions")}</TableHead>
            )}
          </TableRow>
        </TableHeader>
        <TableBody>
          {ordered.map((channel) => (
            <ChannelRow
              busy={bind.isPending || retire.isPending}
              channel={channel}
              equipment={equipment.data}
              key={channel.id}
              profiles={profiles.data ?? []}
              onBind={(binding) => {
                bind.mutate({ key: channel.channelKey, binding });
              }}
              onRetire={() => {
                retire.mutate(channel.channelKey);
              }}
              readOnly={readOnly}
            />
          ))}
        </TableBody>
      </Table>
      {bind.isError || retire.isError ? (
        <p className="text-sm text-destructive" role="alert">
          {t("channels.errors.save")}
        </p>
      ) : null}
    </div>
  );
}
