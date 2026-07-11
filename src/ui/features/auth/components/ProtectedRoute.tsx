import { useSession } from "@/features/auth";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Skeleton,
} from "@/shared/components";
import type { PropsWithChildren } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useLocation } from "react-router";

/** Protected route properties. */
export type ProtectedRouteProps = PropsWithChildren<{
  /** Permission required for the route. */ permission?: string;
}>;

/** Guards a route with session bootstrap and optional permission checks. @param props - Guarded content and permission. @returns Loading, error, redirect, or authorized content. */
export function ProtectedRoute({ children, permission }: ProtectedRouteProps) {
  const session = useSession();
  const location = useLocation();
  const { t } = useTranslation();
  if (session.isPending)
    return (
      <main className="mx-auto flex max-w-screen-xl flex-col gap-4 px-6 py-6">
        <Skeleton className="h-8 w-64" />
        <Skeleton className="h-40 w-full" />
      </main>
    );
  if (session.isError)
    return (
      <main className="mx-auto max-w-screen-xl px-6 py-6">
        <Alert variant="destructive">
          <AlertTitle>{t("auth.bootstrapErrorTitle")}</AlertTitle>
          <AlertDescription>
            {t("auth.bootstrapErrorDescription")}
          </AlertDescription>
        </Alert>
      </main>
    );
  if (!session.data.authenticated)
    return <Navigate replace state={{ from: location.pathname }} to="/login" />;
  if (permission && !session.data.permissions.includes(permission))
    return <Navigate replace to="/forbidden" />;
  return children;
}
