import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { OnboardingPage } from "@/pages/OnboardingPage";
import i18n from "@/shared/lib/i18n";

describe("OnboardingPage", () => {
  it("creates the first system and verifies test ingestion", async () => {
    await i18n.changeLanguage("en");
    const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            systemId,
            credentialSecret: "one-time-secret",
            testEndpoint: "https://pvlog.example/api/test",
          }),
          { status: 201 },
        ),
      )
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({ accepted: true, observedAtEpochMillis: 1000 }),
          { status: 200 },
        ),
      );
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <OnboardingPage />
      </QueryClientProvider>,
    );
    await user.type(screen.getByLabelText("Installation name"), "Home PV");
    await user.click(screen.getByRole("button", { name: "Next" }));
    await user.type(screen.getByLabelText("First system name"), "Roof");
    await user.type(
      screen.getByLabelText("Installed capacity in watts"),
      "6000",
    );
    await user.click(screen.getByRole("button", { name: "Next" }));
    await user.type(
      screen.getByLabelText("Primary inverter or equipment"),
      "Inverter",
    );
    await user.click(
      screen.getByRole("button", { name: "Create system and credential" }),
    );
    expect(await screen.findByText("one-time-secret")).toBeVisible();
    await user.click(
      screen.getByRole("button", { name: "Send and verify test data" }),
    );
    expect(
      await screen.findByText("Test ingestion was accepted and verified."),
    ).toBeVisible();
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });
});
