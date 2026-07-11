import { AppErrorBoundary } from "@/app/AppErrorBoundary";
import { ProtectedRoute } from "@/features/auth";
import {
  ActivationPage,
  AuthCallbackPage,
  ForbiddenPage,
  HomePage,
  LoginPage,
  OnboardingPage,
  RecoveryPage,
} from "@/pages";
import { Alert, AlertDescription, AlertTitle } from "@/shared/components";
import { lazy, Suspense } from "react";
import { useTranslation } from "react-i18next";
import { Route, Routes } from "react-router";

const ApiReferencePage = lazy(async () => {
  const page = await import("@/pages/ApiReferencePage");
  return { default: page.ApiReferencePage };
});

/** Renders public and protected application routes with a localized error boundary. @returns The route tree. */
export function AppRoutes() {
  const { t } = useTranslation();
  const fallback = (
    <main className="mx-auto max-w-screen-xl px-6 py-6">
      <Alert variant="destructive">
        <AlertTitle>{t("errors.boundaryTitle")}</AlertTitle>
        <AlertDescription>{t("errors.boundaryDescription")}</AlertDescription>
      </Alert>
    </main>
  );
  return (
    <AppErrorBoundary fallback={fallback}>
      <Suspense fallback={null}>
        <Routes>
          <Route element={<ApiReferencePage />} path="/docs/api" />
          <Route element={<LoginPage />} path="/login" />
          <Route element={<RecoveryPage />} path="/recovery" />
          <Route element={<ActivationPage />} path="/activate" />
          <Route element={<AuthCallbackPage />} path="/auth/callback" />
          <Route element={<ForbiddenPage />} path="/forbidden" />
          <Route
            element={
              <ProtectedRoute permission="systems:write">
                <OnboardingPage />
              </ProtectedRoute>
            }
            path="/onboarding"
          />
          <Route
            element={
              <ProtectedRoute>
                <HomePage />
              </ProtectedRoute>
            }
            path="*"
          />
        </Routes>
      </Suspense>
    </AppErrorBoundary>
  );
}
