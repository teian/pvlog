/* eslint-disable max-lines-per-function -- waiting, test and verified states share one polling query */
import { useSendTestReading, type DataSource } from "@/entities/dataSource";
import { DiscoveryProposals } from "@/entities/equipmentDiscovery";
import { useFirstData } from "@/entities/system";
import { SetupReadingSummary } from "@/features/systemSetup/components/SetupReadingSummary";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
} from "@/shared/components";
import { CheckCircle2Icon, LoaderCircleIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

/** How often the setup asks whether the first reading has arrived. */
export const FIRST_DATA_POLL_INTERVAL_MILLIS = 3_000;

/** Third setup step: wait for the first reading and show it. @param props - The system, the source that was just connected, and the callback that ends the setup. @returns A live waiting state that turns into the received reading. */
export function SetupVerifyStep({
  systemId,
  dataSource,
  onFinish,
}: {
  systemId: string;
  dataSource: DataSource | undefined;
  onFinish: () => void;
}) {
  const { t } = useTranslation();
  const firstData = useFirstData(systemId, FIRST_DATA_POLL_INTERVAL_MILLIS);
  const test = useSendTestReading(systemId);
  const reading = firstData.data?.reading ?? null;

  if (firstData.data?.received && reading)
    return (
      <div className="flex flex-col gap-5">
        <Alert>
          <CheckCircle2Icon className="text-success" />
          <AlertTitle>{t("setup.verify.receivedTitle")}</AlertTitle>
          <AlertDescription>
            {t("setup.verify.receivedDescription")}
          </AlertDescription>
        </Alert>
        <SetupReadingSummary reading={reading} />
        <DiscoveryProposals readOnly={false} systemId={systemId} />
        <Button className="self-start" onClick={onFinish} type="button">
          {t("setup.verify.openSystem")}
        </Button>
      </div>
    );

  return (
    <div className="flex flex-col gap-5">
      <div
        aria-live="polite"
        className="flex items-center gap-3 rounded-md border p-4 text-sm"
        role="status"
      >
        <LoaderCircleIcon
          aria-hidden="true"
          className="animate-spin text-primary motion-reduce:animate-none"
        />
        <span>
          <span className="block font-semibold">
            {t("setup.verify.waitingTitle")}
          </span>
          <span className="text-muted-foreground">
            {t("setup.verify.waitingDescription")}
          </span>
        </span>
      </div>
      {reading?.test ? (
        <section
          aria-label={t("setup.verify.testTitle")}
          className="flex flex-col gap-3"
        >
          <Alert>
            <AlertTitle>{t("setup.verify.testTitle")}</AlertTitle>
            <AlertDescription>
              {t("setup.verify.testDescription")}
            </AlertDescription>
          </Alert>
          <SetupReadingSummary reading={reading} />
        </section>
      ) : null}
      {firstData.isError ? (
        <p className="text-sm text-destructive" role="alert">
          {t("setup.verify.loadError")}
        </p>
      ) : null}
      {test.isError ? (
        <p className="text-sm text-destructive" role="alert">
          {t("setup.verify.testError")}
        </p>
      ) : null}
      <div className="flex flex-wrap gap-3">
        {dataSource ? (
          <Button
            disabled={test.isPending}
            onClick={() => {
              test.mutate(dataSource.id);
            }}
            type="button"
            variant="outline"
          >
            {t("setup.verify.sendTest")}
          </Button>
        ) : null}
        <Button onClick={onFinish} type="button" variant="outline">
          {t("setup.verify.finishWithoutData")}
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">
        <a className="underline" href="/docs/api">
          {t("setup.verify.docsLink")}
        </a>
      </p>
    </div>
  );
}
