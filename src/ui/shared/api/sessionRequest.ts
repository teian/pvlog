const csrfStorageKey = "pvlog.csrf-token";

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
  if (!response.ok)
    throw new Error(`request_failed:${String(response.status)}`);
  rememberCsrfToken(response);
  return response.status === 204 ? null : response.json();
}
