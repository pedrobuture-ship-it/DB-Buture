import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc, type ConnectInput } from "@/lib/ipc";
import { useUi } from "@/stores/ui";
import type { AppError } from "@/types/schema";

export const keys = {
  status: ["status"] as const,
  saved: ["saved-connections"] as const,
  schema: (system: boolean) => ["schema", system] as const,
};

export function useStatus() {
  return useQuery({ queryKey: keys.status, queryFn: ipc.connectionStatus, staleTime: Infinity });
}

export function useSavedConnections() {
  return useQuery({ queryKey: keys.saved, queryFn: ipc.listConnections });
}

export function useSchema(enabled = true) {
  const showSystem = useUi((s) => s.showSystemSchemas);
  return useQuery({
    queryKey: keys.schema(showSystem),
    queryFn: () => ipc.loadSchema(false, showSystem),
    enabled,
    staleTime: Infinity,
  });
}

export function useRefreshSchema() {
  const qc = useQueryClient();
  const showSystem = useUi((s) => s.showSystemSchemas);
  return useMutation<unknown, AppError>({
    mutationFn: async () => {
      const payload = await ipc.loadSchema(true, showSystem);
      qc.setQueryData(keys.schema(showSystem), payload);
      // derived analyses depend on the schema
      await qc.invalidateQueries({ queryKey: ["analysis"] });
      return payload;
    },
  });
}

export function useConnect() {
  const qc = useQueryClient();
  return useMutation<unknown, AppError, ConnectInput>({
    mutationFn: async (input) => {
      const info = await ipc.connect(input);
      useUi.getState().reset();
      qc.removeQueries({ queryKey: ["schema"] });
      qc.removeQueries({ queryKey: ["analysis"] });
      qc.setQueryData(keys.status, info);
      return info;
    },
  });
}

export function useDisconnect() {
  const qc = useQueryClient();
  return useMutation<unknown, AppError>({
    mutationFn: async () => {
      await ipc.disconnect();
      qc.clear();
      qc.setQueryData(keys.status, null);
      useUi.getState().reset();
    },
  });
}
