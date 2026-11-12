import { DiscoveryAdoptionForm } from "@/entities/equipmentDiscovery/DiscoveryAdoptionForm";
import type {
  DiscoveryAdoption,
  DiscoveryProposal,
} from "@/entities/equipmentDiscovery/equipmentDiscovery.types";
import {
  Badge,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Shows one reported device with what is known about it and, for someone who may manage the system, the form to adopt or dismiss it. @param props - The proposal, whether it may only be viewed, whether a request runs, and the adopt and dismiss callbacks. @returns A card for the device. */
export function DiscoveryProposalCard({
  proposal,
  readOnly,
  busy,
  onAdopt,
  onDismiss,
}: {
  proposal: DiscoveryProposal;
  readOnly: boolean;
  busy: boolean;
  onAdopt: (adoption: DiscoveryAdoption) => void;
  onDismiss: () => void;
}) {
  const { t } = useTranslation();
  const title =
    proposal.reportedName ??
    t("discovery.inverterTitle", { serial: proposal.deviceSerial });
  const facts: [label: string, value: string | null][] = [
    [t("discovery.serial"), proposal.deviceSerial],
    [t("discovery.model"), proposal.model],
    [t("discovery.firmware"), proposal.firmware],
  ];
  return (
    <Card aria-label={title} role="group">
      <CardHeader className="flex flex-row flex-wrap items-center gap-2">
        <CardTitle className="text-base">{title}</CardTitle>
        <Badge variant="outline">
          {t("discovery.channelCount", { count: proposal.channelCount })}
        </Badge>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          {facts.map(([label, value]) =>
            value ? (
              <div className="contents" key={label}>
                <dt className="text-muted-foreground">{label}</dt>
                <dd className="font-mono">{value}</dd>
              </div>
            ) : null,
          )}
        </dl>
        {readOnly ? null : (
          <DiscoveryAdoptionForm
            busy={busy}
            onAdopt={onAdopt}
            onDismiss={onDismiss}
            proposal={proposal}
          />
        )}
      </CardContent>
    </Card>
  );
}
