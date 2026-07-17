import {
  createRole,
  assignRole,
  fetchUserRoleAssignments,
  fetchAuditEvents,
  fetchConnectors,
  fetchLinkedIdentities,
  fetchInverters,
  fetchManagedResources,
  fetchOperationalSummary,
  fetchRoles,
  inviteUser,
  fetchAlertRules,
  fetchWebhooks,
  updateAlertRule,
  fetchAdministrationUsers,
  deleteAdministrationUser,
  fetchWeatherFeedSettings,
  saveWeatherFeedSettings,
  fetchEmailNotificationSettings,
  saveEmailNotificationSettings,
  fetchRetentionBackupSettings,
  saveRetentionBackupSettings,
  runBackup,
} from "@/features/administration/api/administrationApi";
import type {
  AlertRule,
  EmailNotificationSettings,
  RetentionBackupSettings,
  WeatherFeedSettings,
} from "@/features/administration/types/administration.types";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

/** Loads identity links for the active browser session. @returns The identity query state. */
export function useLinkedIdentities() {
  return useQuery({
    queryKey: ["administration", "identities"],
    queryFn: fetchLinkedIdentities,
    retry: false,
  });
}

/** Loads the role catalog for the active account or the instance. @param accountId - Optional active account. @returns The role query state. */
export function useRoles(accountId: string | null | undefined) {
  return useQuery({
    queryKey: ["administration", "roles", accountId],
    queryFn: () => fetchRoles(accountId ?? null),
    retry: false,
  });
}

/** Loads audit events when an account is active. @param accountId - Optional active account. @returns The audit query state. */
export function useAuditEvents(accountId: string | null | undefined) {
  return useQuery({
    queryKey: ["administration", "audit-events", accountId],
    queryFn: () => fetchAuditEvents(accountId ?? ""),
    enabled: Boolean(accountId),
    retry: false,
  });
}

/** Creates a role and refreshes the active account's role catalog. @param accountId - Active account. @returns The mutation state. */
export function useCreateRole(accountId: string | null | undefined) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: { name: string; permissions: string[] }) =>
      createRole(accountId ?? "", input),
    onSuccess: async () =>
      queryClient.invalidateQueries({
        queryKey: ["administration", "roles", accountId],
      }),
  });
}

/** Creates an instance-admin invitation without retaining its one-time token in shared cache. @returns The invitation mutation state. */
export function useInviteUser() {
  return useMutation({ mutationFn: inviteUser });
}

/** Loads the non-secret connector catalog when the browser session has instance-admin access. @returns The connector query state. */
export function useConnectors() {
  return useQuery({
    queryKey: ["administration", "connectors"],
    queryFn: fetchConnectors,
    retry: false,
  });
}

/** Assigns an existing account or instance role and refreshes the affected scope. @param accountId - Active account, or null for the instance. @returns The assignment mutation state. */
export function useAssignRole(accountId: string | null | undefined) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: {
      roleId: string;
      principalType: "user" | "api_credential";
      principalId: string;
      systemId?: string;
    }) => assignRole(accountId ?? null, input),
    onSuccess: async (_assignment, input) =>
      Promise.all([
        queryClient.invalidateQueries({
          queryKey: ["administration", "roles", accountId],
        }),
        queryClient.invalidateQueries({
          queryKey: [
            "administration",
            "role-assignments",
            accountId,
            input.principalId,
          ],
        }),
      ]),
  });
}

/** Loads active role assignments for one user in the current account or instance. @param accountId - Active account, or null for the instance. @param userId - User principal. @returns The assignment query state. */
export function useUserRoleAssignments(
  accountId: string | null | undefined,
  userId: string,
) {
  return useQuery({
    queryKey: ["administration", "role-assignments", accountId, userId],
    queryFn: () => fetchUserRoleAssignments(accountId ?? null, userId),
    retry: false,
  });
}

/** Loads system aggregate and auxiliary resource administration data. @param accountId - Active account. @param systemId - Active system. @returns Parallel resource queries. */
export function useSystemResources(
  accountId: string | null | undefined,
  systemId: string | null | undefined,
) {
  const enabled = Boolean(accountId && systemId);
  const root = `/api/v1/accounts/${accountId ?? ""}`;
  const systemRoot = `${root}/systems/${systemId ?? ""}`;
  return {
    inverters: useQuery({
      queryKey: ["administration", "inverters", accountId, systemId],
      queryFn: () => fetchInverters(systemId ?? ""),
      enabled,
      retry: false,
    }),
    equipment: useQuery({
      queryKey: ["administration", "equipment", accountId, systemId],
      queryFn: () => fetchManagedResources(`${systemRoot}/equipment`),
      enabled,
      retry: false,
    }),
    tariffs: useQuery({
      queryKey: ["administration", "tariffs", accountId, systemId],
      queryFn: () => fetchManagedResources(`${systemRoot}/tariffs`),
      enabled,
      retry: false,
    }),
    channels: useQuery({
      queryKey: ["administration", "channels", accountId, systemId],
      queryFn: () => fetchManagedResources(`${systemRoot}/channels`),
      enabled,
      retry: false,
    }),
    memberships: useQuery({
      queryKey: ["administration", "memberships", accountId],
      queryFn: () => fetchManagedResources(`${root}/memberships`),
      enabled: Boolean(accountId),
      retry: false,
    }),
    credentials: useQuery({
      queryKey: ["administration", "credentials", accountId],
      queryFn: () => fetchManagedResources(`${root}/credentials`),
      enabled: Boolean(accountId),
      retry: false,
    }),
  };
}

/** Loads safe operational administration counts. @param accountId - Active account. @returns Operational summary query. */
export function useOperationalSummary(accountId: string | null | undefined) {
  return useQuery({
    queryKey: ["administration", "operations", accountId],
    queryFn: () => fetchOperationalSummary(accountId ?? ""),
    enabled: Boolean(accountId),
    retry: false,
  });
}

/** Loads alert rules when an account is active. */
export function useAlertRules(accountId: string | null | undefined) {
  return useQuery({
    queryKey: ["administration", "alert-rules", accountId],
    queryFn: () => fetchAlertRules(accountId ?? ""),
    enabled: Boolean(accountId),
    retry: false,
  });
}

/** Updates one alert rule and refreshes the account rule list. */
export function useUpdateAlertRule(accountId: string | null | undefined) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (rule: AlertRule) => updateAlertRule(accountId ?? "", rule),
    onSuccess: async () =>
      queryClient.invalidateQueries({
        queryKey: ["administration", "alert-rules", accountId],
      }),
  });
}

/** Loads webhook notification channels when an account is active. */
export function useWebhooks(accountId: string | null | undefined) {
  return useQuery({
    queryKey: ["administration", "webhooks", accountId],
    queryFn: () => fetchWebhooks(accountId ?? ""),
    enabled: Boolean(accountId),
    retry: false,
  });
}

export function useAdministrationUsers() {
  return useQuery({
    queryKey: ["administration", "users"],
    queryFn: fetchAdministrationUsers,
    retry: false,
  });
}

/** Deletes an eligible local user and refreshes the administrator directory. @returns The deletion mutation state. */
export function useDeleteAdministrationUser() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: deleteAdministrationUser,
    onSuccess: async () =>
      queryClient.invalidateQueries({
        queryKey: ["administration", "users"],
      }),
  });
}

export function useWeatherFeedSettings() {
  return useQuery({
    queryKey: ["administration", "weather-feed"],
    queryFn: fetchWeatherFeedSettings,
    retry: false,
  });
}

export function useSaveWeatherFeedSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (settings: WeatherFeedSettings) =>
      saveWeatherFeedSettings(settings),
    onSuccess: async () =>
      queryClient.invalidateQueries({
        queryKey: ["administration", "weather-feed"],
      }),
  });
}

export function useEmailNotificationSettings() {
  return useQuery({
    queryKey: ["administration", "email-notifications"],
    queryFn: fetchEmailNotificationSettings,
    retry: false,
  });
}

export function useSaveEmailNotificationSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (settings: EmailNotificationSettings) =>
      saveEmailNotificationSettings(settings),
    onSuccess: async () =>
      queryClient.invalidateQueries({
        queryKey: ["administration", "email-notifications"],
      }),
  });
}

export function useRetentionBackupSettings() {
  return useQuery({
    queryKey: ["administration", "retention-backup"],
    queryFn: fetchRetentionBackupSettings,
    retry: false,
  });
}

export function useSaveRetentionBackupSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (settings: RetentionBackupSettings) =>
      saveRetentionBackupSettings(settings),
    onSuccess: async () =>
      queryClient.invalidateQueries({
        queryKey: ["administration", "retention-backup"],
      }),
  });
}

export function useRunBackup() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: runBackup,
    onSuccess: async () =>
      queryClient.invalidateQueries({
        queryKey: ["administration", "retention-backup"],
      }),
  });
}
