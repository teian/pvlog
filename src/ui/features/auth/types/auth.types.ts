import { z } from "zod";

/** Validated external connector metadata displayed by the login page. */
export const authConnectorSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  authorizationUrl: z.url(),
});

/** Validated browser session bootstrap response. */
export const sessionSchema = z.object({
  authenticated: z.boolean(),
  user: z.object({ id: z.uuid(), displayName: z.string() }).nullable(),
  accountId: z.uuid().nullable(),
  systemIds: z.array(z.uuid()),
  permissions: z.array(z.string()),
  connectors: z.array(authConnectorSchema),
});

/** Browser session returned by the backend bootstrap endpoint. */
export type Session = z.infer<typeof sessionSchema>;
