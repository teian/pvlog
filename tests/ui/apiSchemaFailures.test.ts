import { afterEach, describe, expect, it, vi } from "vitest";

import { fetchInverters } from "@/features/administration/api/administrationApi";

describe("API response schema validation", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("rejects a malformed inverter response at the API boundary", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        Response.json([{ id: "not-a-uuid", strings: "invalid" }]),
      ),
    );

    await expect(
      fetchInverters("019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71"),
    ).rejects.toThrow();
  });
});
