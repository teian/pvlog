import { describe, expect, it } from "vitest";

import { retryQueryOnce } from "@/shared/api/queryRetry";
import { SessionRequestError } from "@/shared/api/sessionRequest";

describe("retryQueryOnce", () => {
  it("retries a server or network failure once", () => {
    expect(retryQueryOnce(0, new SessionRequestError(503, null, null))).toBe(
      true,
    );
    expect(retryQueryOnce(0, new Error("series_failed:500"))).toBe(true);
    expect(retryQueryOnce(0, new TypeError("Failed to fetch"))).toBe(true);
    expect(retryQueryOnce(1, new Error("series_failed:500"))).toBe(false);
  });

  it("does not retry a request the server rejected", () => {
    expect(retryQueryOnce(0, new SessionRequestError(404, null, null))).toBe(
      false,
    );
    expect(retryQueryOnce(0, new Error("forecast_settings_failed:404"))).toBe(
      false,
    );
    expect(retryQueryOnce(0, new Error("series_failed:403"))).toBe(false);
  });
});
