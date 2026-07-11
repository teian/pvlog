import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/app";
import i18n from "@/shared/lib/i18n";

describe("invitation activation workflow", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    window.history.replaceState({}, "", "/activate?token=one-time-token");
  });

  it("submits the invitation token, display name, and initial password to the real acceptance endpoint", async () => {
    const fetchMock = vi.fn(
      async () =>
        new Response(JSON.stringify({ status: "accepted" }), {
          status: 202,
          headers: { "content-type": "application/json" },
        }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    render(<App runtimeConfig={runtimeConfig} />);

    await user.type(screen.getByLabelText("Display name"), "Invitee");
    await user.type(screen.getByLabelText("New password"), "accepted-password");
    await user.click(screen.getByRole("button", { name: "Activate account" }));

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/auth/invitations/accept",
      expect.objectContaining({
        body: JSON.stringify({
          token: "one-time-token",
          displayName: "Invitee",
          password: "accepted-password",
        }),
        method: "POST",
      }),
    );
    expect(
      await screen.findByText("Your account is active. You can now sign in."),
    ).toBeVisible();
  });
});

const runtimeConfig = {
  apiBaseUrl: "/api",
  telemetry: {
    enabled: false,
    headers: {},
    serviceName: "pvlog-ui",
    serviceVersion: "test",
  },
};
