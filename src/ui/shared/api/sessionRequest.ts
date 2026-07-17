const csrfStorageKey = "pvlog.csrf-token";

/** Structured failure returned by an authenticated JSON request. */
export class SessionRequestError extends Error {
  readonly status: number;
  readonly detail: string | null;
  readonly requestId: string | null;

  /** @param status - HTTP response status. @param detail - Optional safe problem detail. @param requestId - Optional correlation identifier. */
  constructor(status: number, detail: string | null, requestId: string | null) {
    super(`request_failed:${String(status)}`);
    this.name = "SessionRequestError";
    this.status = status;
    this.detail = detail;
    this.requestId = requestId;
  }
}

async function requestError(response: Response): Promise<SessionRequestError> {
  let detail: string | null = null;
  let requestId: string | null = null;
  if (response.headers.get("content-type")?.includes("json")) {
    const problem: unknown = await response.json().catch(() => null);
    if (problem && typeof problem === "object") {
      const record = problem as Record<string, unknown>;
      detail = typeof record.detail === "string" ? record.detail : null;
      requestId =
        typeof record.requestId === "string" ? record.requestId : null;
    }
  }
  return new SessionRequestError(response.status, detail, requestId);
}

function csrfToken(): string | null {
  return typeof window === "undefined"
    ? null
    : window.sessionStorage.getItem(csrfStorageKey);
}

function rememberCsrfToken(response: Response): void {
  const token = response.headers.get("x-csrf-token");
  if (token && typeof window !== "undefined")
    window.sessionStorage.setItem(csrfStorageKey, token);
}

/** Sends a same-origin JSON request, attaching the session CSRF token to state-changing requests. @param path - Relative API path. @param init - Request options. @returns The parsed JSON body, or null for an empty response. */
export async function sessionJsonRequest(
  path: string,
  init?: RequestInit,
): Promise<unknown> {
  const headers = new Headers(init?.headers);
  headers.set("content-type", "application/json");
  if (!["GET", "HEAD", "OPTIONS"].includes(init?.method ?? "GET")) {
    const token = csrfToken();
    if (token) headers.set("x-csrf-token", token);
  }
  const response = await fetch(path, {
    credentials: "same-origin",
    ...init,
    headers,
  });
  if (!response.ok) throw await requestError(response);
  rememberCsrfToken(response);
  return response.status === 204 ? null : response.json();
}
