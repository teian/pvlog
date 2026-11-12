import type { DiscoveryProposal } from "@/entities/equipmentDiscovery/equipmentDiscovery.types";
import { Button } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Lists dismissed devices so that one can be proposed again. @param props - The dismissed proposals, whether a request runs, and the reset callback. @returns A list with one action per device, or nothing when no device is dismissed. */
export function DismissedProposals({
  proposals,
  busy,
  onReset,
}: {
  proposals: DiscoveryProposal[];
  busy: boolean;
  onReset: (proposalId: string) => void;
}) {
  const { t } = useTranslation();
  if (proposals.length === 0) return null;
  return (
    <ul
      aria-label={t("discovery.dismissedTitle")}
      className="flex flex-col gap-2 text-sm"
    >
      {proposals.map((proposal) => (
        <li
          className="flex flex-wrap items-center gap-3 text-muted-foreground"
          key={proposal.id}
        >
          <span>
            {t("discovery.dismissedItem", {
              name:
                proposal.reportedName ??
                t("discovery.inverterTitle", {
                  serial: proposal.deviceSerial,
                }),
            })}
          </span>
          <Button
            disabled={busy}
            onClick={() => {
              onReset(proposal.id);
            }}
            size="sm"
            type="button"
            variant="outline"
          >
            {t("discovery.proposeAgain")}
          </Button>
        </li>
      ))}
    </ul>
  );
}
