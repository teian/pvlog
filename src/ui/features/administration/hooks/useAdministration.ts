import {
  createRole,
  assignRole,
  fetchAuditEvents,
  fetchConnectors,
  fetchLinkedIdentities,
  fetchInverters,
  fetchManagedResources,
  fetchOperationalSummary,
  fetchRoles,
  inviteUser,
} from "@/features/administration/api/administrationApi";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

/** Loads identity links for the active browser session. @returns The identity query state. */
export function useLinkedIdentities() {
  return useQuery({
    queryKey: ["administration", "identities"],
    queryFn: fetchLinkedIdentities,
    retry: false,
  });
}

/** Loads the role catalog when an account is active. @param accountId - Optional active account. @returns The role query state. */
export function useRoles(accountId: string | null | undefined) {
  return useQuery({
    queryKey: ["administration", "roles", accountId],
    queryFn: () => fetchRoles(accountId ?? ""),
    enabled: Boolean(accountId),
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

/** Assigns an existing account role and refreshes the role catalog for the affected account. @param accountId - Active account. @returns The assignment mutation state. */
export function useAssignRole(accountId: string | null | undefined) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: {
      roleId: string;
      principalType: "user" | "api_credential";
      principalId: string;
      systemId?: string;
    }) => assignRole(accountId ?? "", input),
    onSuccess: async () =>
      queryClient.invalidateQueries({
        queryKey: ["administration", "roles", accountId],
      }),
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
      queryFn: () => fetchInverters(accountId ?? "", systemId ?? ""),
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
