import { SessionRequestError } from "@/shared/api/sessionRequest";

/** HTTP status a failed request carries, from a session request error or a `name_failed:<status>` message. @param error - Query failure. @returns The status, or null when the failure has none. */
function failedStatus(error: unknown): number | null {
  if (error instanceof SessionRequestError) return error.status;
  const match =
    error instanceof Error ? /:(\d{3})$/u.exec(error.message) : null;
  return match ? Number(match[1]) : null;
}

/**
 * Retries a failed query once, unless the server rejected the request itself:
 * asking again for something missing or forbidden only delays the page.
 *
 * @param failureCount - Failures so far.
 * @param error - The latest failure.
 * @returns Whether to try again.
 */
export function retryQueryOnce(failureCount: number, error: unknown): boolean {
  if (failureCount >= 1) return false;
  const status = failedStatus(error);
  return status === null || status < 400 || status >= 500;
}
