import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";

import { HomePage } from "@/pages/HomePage";
import i18n from "@/shared/lib/i18n";

describe("HomePage", () => {
  it("renders the English localized application identity", async () => {
    await i18n.changeLanguage("en");
    render(
      <QueryClientProvider client={new QueryClient()}>
        <MemoryRouter>
          <HomePage />
        </MemoryRouter>
      </QueryClientProvider>,
    );

    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      "PVLog",
    );
    expect(
      screen.getByText("Self-hosted photovoltaic monitoring."),
    ).toBeVisible();
  });
});
