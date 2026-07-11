import { requestAnalysisExport } from "@/features/charts/api/chartsApi";
import { downloadBlob } from "@/features/charts/utils/downloadBlob";
import { useMutation } from "@tanstack/react-query";

/** Requests a chart-matching CSV/JSON export and downloads it once ready, or reports a queued job. @returns The export mutation state. */
export function useAnalysisExport() {
  return useMutation({
    mutationFn: requestAnalysisExport,
    onSuccess: (result) => {
      if (result.kind === "file") downloadBlob(result.blob, result.filename);
    },
  });
}
