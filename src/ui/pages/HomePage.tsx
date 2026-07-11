import { useSession } from "@/features/auth";
import { DashboardPage } from "@/pages/DashboardPage";
import { AppShell } from "@/widgets";

/**
 * Displays the initial application placeholder while vertical slices are added.
 *
 * @returns The accessible initial page.
 */
export function HomePage() {
  const session = useSession();

  return (
    <AppShell
      accountId={session.data?.accountId}
      systemIds={session.data?.systemIds}
    >
      <DashboardPage />
    </AppShell>
  );
}
