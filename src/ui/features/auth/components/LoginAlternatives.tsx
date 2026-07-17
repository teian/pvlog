import { Button, Separator } from "@/shared/components";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import type { AuthConnector } from "../types/auth.types";

/** Alternative login method properties. */
export interface LoginAlternativesProps {
  /** Configured external identity providers. */
  connectors: AuthConnector[];
}

/** Renders external login choices and account recovery. @param props - Available external connectors. @returns Login alternatives. */
export function LoginAlternatives({ connectors }: LoginAlternativesProps) {
  const { t } = useTranslation();

  return (
    <>
      {connectors.length > 0 ? (
        <>
          <div className="my-7 flex items-center gap-3">
            <Separator className="flex-1" />
            <span className="text-xs text-muted-foreground">
              {t("auth.or")}
            </span>
            <Separator className="flex-1" />
          </div>
          <div className="flex flex-col gap-2">
            {connectors.map((connector) => (
              <Button
                asChild
                className="w-full"
                key={connector.id}
                variant="outline"
              >
                <a href={connector.authorizationUrl}>
                  {t("auth.signInWith", { name: connector.name })}
                </a>
              </Button>
            ))}
          </div>
        </>
      ) : null}

      <div className="mt-5 text-center">
        <Button asChild size="sm" variant="link">
          <Link to="/recovery">{t("auth.forgotPassword")}</Link>
        </Button>
      </div>
    </>
  );
}
