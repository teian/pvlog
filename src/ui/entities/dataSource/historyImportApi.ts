import {
  historyImportSchema,
  type HistoryImport,
} from "@/entities/dataSource/historyImport.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

const imports = (systemId: string, dataSourceId: string) =>
  `/api/v1/systems/${systemId}/data-sources/${dataSourceId}/imports`;

/** Lists the dry runs and imports of an import source. @param systemId - Owning system. @param dataSourceId - Import source. @returns Imports, newest first. */
export async function fetchHistoryImports(
  systemId: string,
  dataSourceId: string,
): Promise<HistoryImport[]> {
  return z
    .array(historyImportSchema)
    .parse(await sessionJsonRequest(imports(systemId, dataSourceId)));
}

/** Compresses a request body with gzip where the browser can. @param body - JSON text. @returns The body and its encoding. */
async function compressed(
  body: string,
): Promise<{ body: BodyInit; encoding: string | null }> {
  const plain = new Response(body).body;
  if (typeof CompressionStream === "undefined" || plain === null)
    return { body, encoding: null };
  const stream = plain.pipeThrough(new CompressionStream("gzip"));
  return { body: await new Response(stream).arrayBuffer(), encoding: "gzip" };
}

/**
 * Uploads a PVOutput export for a dry run; nothing is stored as telemetry yet.
 *
 * @param systemId - Owning system.
 * @param dataSourceId - Import source that takes the file.
 * @param file - The export as the browser read it.
 * @returns The dry run with its report.
 */
export async function uploadHistoryImport(
  systemId: string,
  dataSourceId: string,
  file: File,
): Promise<HistoryImport> {
  const { body, encoding } = await compressed(
    JSON.stringify({ fileName: file.name, content: await file.text() }),
  );
  return historyImportSchema.parse(
    await sessionJsonRequest(imports(systemId, dataSourceId), {
      method: "POST",
      body,
      headers: encoding ? { "content-encoding": encoding } : {},
    }),
  );
}

/** Stores what a dry run accepted; committing again changes nothing. @param systemId - Owning system. @param dataSourceId - Import source. @param importId - Dry run to commit. @returns The committed import with what was stored. */
export async function commitHistoryImport(
  systemId: string,
  dataSourceId: string,
  importId: string,
): Promise<HistoryImport> {
  return historyImportSchema.parse(
    await sessionJsonRequest(
      `${imports(systemId, dataSourceId)}/${importId}/commit`,
      { method: "POST" },
    ),
  );
}
