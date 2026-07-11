import { sessionSchema, type Session } from "@/features/auth/types/auth.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";

/** Loads the current user, permissions, systems, and connector choices. @returns The validated session bootstrap. */
export async function fetchSession(): Promise<Session> {
  return sessionSchema.parse(await sessionJsonRequest("/api/v1/session"));
}

/** Authenticates with a local credential. @param email - Local account email. @param password - Local password. @returns The refreshed session. */
export async function login(email: string, password: string): Promise<Session> {
  return sessionSchema.parse(
    await sessionJsonRequest("/api/v1/auth/local/login", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),
  );
}

/** Starts local password recovery. @param email - Local account email. @returns Completion after the enumeration-safe request. */
export async function requestRecovery(email: string): Promise<void> {
  await sessionJsonRequest("/api/v1/auth/password-recovery", {
    method: "POST",
    body: JSON.stringify({ email }),
  });
}

/** Accepts an invitation and stores the invited user's initial local password. @param token - One-time invitation token. @param displayName - Invited user's display name. @param password - Initial password. @returns Completion after acceptance. */
export async function activate(
  token: string,
  displayName: string,
  password: string,
): Promise<void> {
  await sessionJsonRequest("/api/v1/auth/invitations/accept", {
    method: "POST",
    body: JSON.stringify({ token, displayName, password }),
  });
}

/** Revokes the active browser session. @returns Completion after the server-side session revocation. */
export async function logout(): Promise<void> {
  await sessionJsonRequest("/api/v1/session", { method: "POST" });
}
