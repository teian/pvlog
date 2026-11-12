import { z } from "zod";

/** Largest PVOutput export the server reads, in bytes. */
export const MAXIMUM_IMPORT_FILE_BYTES = 4 * 1024 * 1024;

const count = z.number().int().nonnegative();

/** A row of an import file that is not imported, with the reason. */
export const historyImportRejectionSchema = z.object({
  row: z.number().int().positive(),
  reason: z.string(),
  column: z.string().optional(),
});

/** What a dry run found in a file. */
export const historyImportReportSchema = z.object({
  rows: count,
  accepted: count,
  rejectedCount: count,
  rejected: z.array(historyImportRejectionSchema),
  firstDate: z.string().nullable().default(null),
  lastDate: z.string().nullable().default(null),
  overlap: z.object({ alreadyImported: count, otherSources: count }),
});

/** A dry run of a PVOutput export, or the import that committed it. */
export const historyImportSchema = z.object({
  id: z.uuid(),
  dataSourceId: z.uuid(),
  fileName: z.string(),
  kind: z.enum(["status", "output"]),
  state: z.enum(["ready", "committed"]),
  createdAtEpochMillis: z.number().int(),
  expiresAtEpochMillis: z.number().int().optional(),
  committedAtEpochMillis: z.number().int().optional(),
  report: historyImportReportSchema,
  result: z
    .object({ stored: count, alreadyPresent: count, skipped: count })
    .optional(),
});

export type HistoryImportRejection = z.infer<
  typeof historyImportRejectionSchema
>;
export type HistoryImportReport = z.infer<typeof historyImportReportSchema>;
export type HistoryImport = z.infer<typeof historyImportSchema>;
