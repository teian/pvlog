import {
  createAccountApiKey,
  fetchAccountApiKeys,
  revokeAccountApiKey,
} from "@/features/accountApiKeys/api/accountApiKeysApi";
import type {
  AccountApiKey,
  CreateAccountApiKeyInput,
} from "@/features/accountApiKeys/types/accountApiKeys.types";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

const queryKey = ["account", "api-keys"] as const;

/** Loads current-account API-key metadata with no secret material. @returns Account API-key query state. */
export function useAccountApiKeys() {
  return useQuery({
    queryKey,
    queryFn: fetchAccountApiKeys,
    staleTime: 0,
  });
}

/** Creates a key while diverting its secret out of TanStack Query's mutation cache. @param onIssued - Ephemeral one-time secret consumer. @returns Creation mutation containing metadata only. */
export function useCreateAccountApiKey(
  onIssued: (apiKey: string, credential: AccountApiKey) => void,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (input: CreateAccountApiKeyInput) => {
      const issued = await createAccountApiKey(input);
      onIssued(issued.apiKey, issued.credential);
      return issued.credential;
    },
    onSuccess: (credential) => {
      queryClient.setQueryData<AccountApiKey[]>(queryKey, (current = []) => [
        credential,
        ...current,
      ]);
    },
  });
}

/** Revokes a key and refreshes safe account metadata. @returns Revocation mutation state. */
export function useRevokeAccountApiKey() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: revokeAccountApiKey,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey });
    },
  });
}
