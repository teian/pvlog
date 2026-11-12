/**
 * Permissions that open each guarded destination; holding any one suffices.
 *
 * Routes and navigation both read this table, so the navigation can never
 * offer an entry whose route would answer with the forbidden page.
 */
export const DESTINATION_PERMISSIONS = {
  systems: ["system_read"],
  setup: ["system_manage"],
  reports: ["telemetry_read"],
  system: ["telemetry_read"],
  administration: [
    "instance_read",
    "instance_manage",
    "account_manage",
    "membership_manage",
    "role_manage",
    "integration_manage",
    "audit_read",
  ],
} as const satisfies Record<string, readonly string[]>;

/** A destination whose access depends on the session's permissions. */
export type GuardedDestination = keyof typeof DESTINATION_PERMISSIONS;

/**
 * Decides whether a session may open a destination.
 *
 * @param permissions - Permissions the session holds.
 * @param destination - Destination to open.
 * @returns True when at least one required permission is held.
 */
export function mayOpen(
  permissions: readonly string[] | undefined,
  destination: GuardedDestination,
): boolean {
  const required: readonly string[] = DESTINATION_PERMISSIONS[destination];
  return (permissions ?? []).some((permission) =>
    required.includes(permission),
  );
}
