import { sessionSchema, type Session } from "@/features/auth/types/auth.types";

async function jsonRequest(path: string, init?: RequestInit): Promise<unknown> {
  const headers = new Headers(init?.headers);
  headers.set("content-type", "application/json");
  const response = await fetch(path, {
    credentials: "same-origin",
    ...init,
    headers,
  });
  if (!response.ok)
    throw new Error(`request_failed:${String(response.status)}`);
  return response.status === 204 ? null : response.json();
}

/** Loads the current user, permissions, systems, and connector choices. @returns The validated session bootstrap. */
export async function fetchSession(): Promise<Session> {
  return sessionSchema.parse(await jsonRequest("/api/v1/session"));
}

/** Authenticates with a local credential. @param email - Local account email. @param password - Local password. @returns The refreshed session. */
export async function login(email: string, password: string): Promise<Session> {
  return sessionSchema.parse(
    await jsonRequest("/api/v1/auth/local/login", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),
  );
}

/** Starts local password recovery. @param email - Local account email. @returns Completion after the enumeration-safe request. */
export async function requestRecovery(email: string): Promise<void> {
  await jsonRequest("/api/v1/auth/local/recovery", {
    method: "POST",
    body: JSON.stringify({ email }),
  });
}

/** Activates an invited local account. @param token - One-time activation token. @param password - Initial password. @returns Completion after activation. */
export async function activate(token: string, password: string): Promise<void> {
  await jsonRequest("/api/v1/auth/local/activation", {
    method: "POST",
    body: JSON.stringify({ token, password }),
  });
}
