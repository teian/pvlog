import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/app";
import i18n from "@/shared/lib/i18n";

describe("authentication workflows", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    window.history.replaceState({}, "", "/login");
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              authenticated: false,
              user: null,
              accountId: null,
              systemIds: [],
              permissions: [],
              connectors: [
                {
                  id: "oidc",
                  name: "Company SSO",
                  authorizationUrl: "https://identity.example/authorize",
                },
              ],
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          ),
      ),
    );
  });

  it("offers local login, recovery, and configured external connectors", async () => {
    render(
      <App
        runtimeConfig={{
          apiBaseUrl: "/api",
          telemetry: {
            enabled: false,
            headers: {},
            serviceName: "pvlog-ui",
            serviceVersion: "test",
          },
        }}
      />,
    );
    expect(
      await screen.findByRole("heading", { name: "Sign in to PVLog" }),
    ).toBeVisible();
    expect(screen.getByLabelText("Email address")).toBeVisible();
    expect(
      await screen.findByRole("link", { name: "Continue with Company SSO" }),
    ).toHaveAttribute("href", "https://identity.example/authorize");
    expect(
      screen.getByRole("link", { name: "Forgot password?" }),
    ).toHaveAttribute("href", "/recovery");
  });
});
