import {
  createRole,
  assignRole,
  fetchAuditEvents,
  fetchConnectors,
  fetchLinkedIdentities,
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
