import {
  AccountProfileForm,
  PasswordChangeForm,
} from "@/features/accountSettings";
import i18n from "@/shared/lib/i18n";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const userId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
let fetchMock: ReturnType<typeof vi.fn>;

describe("account settings", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    window.sessionStorage.setItem("pvlog.csrf-token", "csrf");
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      if (path === "/api/v1/auth/password") {
        return new Response(null, { status: 204 });
      }
      if (init?.method === "PUT") {
        const body = JSON.parse(String(init.body)) as { displayName: string };
        return json({
          id: userId,
          email: "ada@example.test",
          displayName: body.displayName,
        });
      }
      return json({
        id: userId,
        email: "ada@example.test",
        displayName: "Ada Lovelace",
      });
    });
    vi.stubGlobal("fetch", fetchMock);
  });

  it("updates the display name while keeping the login email read-only", async () => {
    const user = userEvent.setup();
    renderSettings();
    const name = await screen.findByLabelText("Name");
    expect(screen.getByLabelText("Email address")).toBeDisabled();
    await user.clear(name);
    await user.type(name, "Ada Byron");
    await user.click(screen.getByRole("button", { name: "Save changes" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "/api/v1/account/profile",
        expect.objectContaining({
          method: "PUT",
          body: JSON.stringify({ displayName: "Ada Byron" }),
        }),
      );
    });
    expect(
      await screen.findByText("Your account details have been saved."),
    ).toBeVisible();
  });

  it("blocks mismatched passwords and sends a valid password change", async () => {
    const user = userEvent.setup();
    renderSettings();
    await user.type(screen.getByLabelText("Current password"), "old-password");
    await user.type(screen.getByLabelText("New password"), "new-password-123");
    await user.type(screen.getByLabelText("Confirm new password"), "different");
    expect(screen.getByText("The new passwords do not match.")).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Change password" }),
    ).toBeDisabled();

    await user.clear(screen.getByLabelText("Confirm new password"));
    await user.type(
      screen.getByLabelText("Confirm new password"),
      "new-password-123",
    );
    await user.click(screen.getByRole("button", { name: "Change password" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "/api/v1/auth/password",
        expect.objectContaining({
          method: "PUT",
          body: JSON.stringify({
            currentPassword: "old-password",
            newPassword: "new-password-123",
          }),
        }),
      );
    });
    expect(
      await screen.findByText("Your password has been changed."),
    ).toBeVisible();
  });
});

function renderSettings() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <AccountProfileForm />
      <PasswordChangeForm />
    </QueryClientProvider>,
  );
}

function json(value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}
