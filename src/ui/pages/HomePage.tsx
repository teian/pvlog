import { useSession } from "@/features/auth";
import { DashboardPage } from "@/pages/DashboardPage";
import { AppShell } from "@/widgets";
import { Navigate } from "react-router";

/**
 * Shows the dashboard, or starts the guided setup for someone who may manage
 * systems and has none yet: an empty dashboard would tell them nothing.
 *
 * @returns The dashboard, or a redirect to the setup.
 */
export function HomePage() {
  const session = useSession();
  const data = session.data;
  if (
    data?.authenticated &&
    data.systemIds.length === 0 &&
    data.permissions.includes("system_manage")
  )
    return <Navigate replace to="/setup" />;

  return (
    <AppShell systemIds={data?.systemIds}>
      <DashboardPage />
    </AppShell>
  );
}
