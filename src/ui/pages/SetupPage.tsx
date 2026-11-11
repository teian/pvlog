import { useSession } from "@/features/auth";
import { SystemSetupWizard } from "@/features/systemSetup";
import { AppShell } from "@/widgets";
import { useNavigate, useParams } from "react-router";

/** Hosts the guided setup for a new system, or resumes it for an existing one. @returns The protected setup route. */
export function SetupPage() {
  const session = useSession();
  const navigate = useNavigate();
  const { systemId } = useParams();
  return (
    <AppShell systemIds={session.data?.systemIds}>
      <SystemSetupWizard
        key={systemId ?? "new"}
        onFinished={(id) => {
          void navigate(`/systems/${id}`);
        }}
        {...(systemId ? { resumeSystemId: systemId } : {})}
      />
    </AppShell>
  );
}
