import { useLogout, useSession } from "@/features/auth";
import { Button } from "@/shared/components";
import { CircleUserRoundIcon, LogOutIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router";

/** Derives stable initials from a display name. */
function getInitials(displayName: string) {
  return displayName
    .split(/\s+/u)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part.at(0)?.toUpperCase())
    .join("");
}

/** Renders the signed-in identity and logout action in the sidebar. @returns The optional session controls. */
export function SessionControls() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const session = useSession();
  const logout = useLogout();
  const user = session.data?.user;

  if (!session.data?.authenticated || !user) return null;

  return (
    <div className="mt-3 flex items-center gap-2 px-2">
      <span
        aria-hidden="true"
        className="flex size-7 shrink-0 items-center justify-center rounded-full bg-primary text-[10px] font-bold text-primary-foreground"
      >
        {getInitials(user.displayName)}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-xs font-semibold">
          {user.displayName}
        </span>
      </span>
      <Button
        aria-label={t("account.navigation")}
        asChild
        className="text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
        size="icon-sm"
        variant="ghost"
      >
        <Link to="/account">
          <CircleUserRoundIcon />
        </Link>
      </Button>
      <Button
        aria-label={t("auth.signOut")}
        className="text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
        disabled={logout.isPending}
        onClick={() => {
          logout.mutate(undefined, {
            onSuccess: () => {
              void navigate("/login", { replace: true });
            },
          });
        }}
        size="icon-sm"
        variant="ghost"
      >
        <LogOutIcon />
      </Button>
    </div>
  );
}
