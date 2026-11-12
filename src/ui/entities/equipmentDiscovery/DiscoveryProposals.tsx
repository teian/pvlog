import { DiscoveryProposalCard } from "@/entities/equipmentDiscovery/DiscoveryProposalCard";
import { DismissedProposals } from "@/entities/equipmentDiscovery/DismissedProposals";
import {
  useAdoptDiscoveryProposal,
  useDiscoveryProposals,
  useDismissDiscoveryProposal,
  useResetDiscoveryProposal,
} from "@/entities/equipmentDiscovery/useEquipmentDiscovery";
import { useId } from "react";
import { useTranslation } from "react-i18next";

/** Offers the devices a data source reported that are not part of the system yet. @param props - The system and whether its equipment may only be viewed. @returns The proposals with their adopt and dismiss actions, or nothing while there is none. */
export function DiscoveryProposals({
  systemId,
  readOnly,
}: {
  systemId: string;
  readOnly: boolean;
}) {
  const { t } = useTranslation();
  const headingId = useId();
  const proposals = useDiscoveryProposals(systemId);
  const adopt = useAdoptDiscoveryProposal(systemId);
  const dismiss = useDismissDiscoveryProposal(systemId);
  const reset = useResetDiscoveryProposal(systemId);
  const all = proposals.data ?? [];
  const open = all.filter((proposal) => proposal.state === "open");
  // A dismissed device is only of interest to someone who can propose it again.
  const dismissed = readOnly
    ? []
    : all.filter((proposal) => proposal.state === "dismissed");
  if (open.length + dismissed.length === 0) return null;
  const busy = adopt.isPending || dismiss.isPending || reset.isPending;
  const failed = adopt.isError || dismiss.isError || reset.isError;
  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-3">
      <div>
        <h2 className="text-sm font-bold" id={headingId}>
          {t("discovery.title")}
        </h2>
        {open.length > 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("discovery.description")}
          </p>
        ) : null}
      </div>
      {open.map((proposal) => (
        <DiscoveryProposalCard
          busy={busy}
          key={proposal.id}
          onAdopt={(adoption) => {
            adopt.mutate({ proposalId: proposal.id, adoption });
          }}
          onDismiss={() => {
            dismiss.mutate(proposal.id);
          }}
          proposal={proposal}
          readOnly={readOnly}
        />
      ))}
      {failed ? (
        <p className="text-sm text-destructive" role="alert">
          {t("discovery.error")}
        </p>
      ) : null}
      <DismissedProposals
        busy={busy}
        onReset={(proposalId) => {
          reset.mutate(proposalId);
        }}
        proposals={dismissed}
      />
    </section>
  );
}
