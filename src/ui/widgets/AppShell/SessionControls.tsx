import { useLogout, useSession } from "@/features/auth";
import { Button } from "@/shared/components";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";

/** Renders a logout control only while an authenticated browser session is active. @returns The optional logout control. */
export function SessionControls() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const session = useSession();
  const logout = useLogout();
  if (!session.data?.authenticated) return null;
  return (
    <Button
      disabled={logout.isPending}
      onClick={() => {
        logout.mutate(undefined, {
          onSuccess: () => {
            void navigate("/login", { replace: true });
          },
        });
      }}
      size="sm"
      variant="ghost"
    >
      {t("auth.signOut")}
    </Button>
  );
}
