import {
  dataSourceSchema,
  issuedDataSourceSchema,
  sourceProfileSchema,
  testReadingSchema,
  type DataSource,
  type IssuedDataSource,
  type SourceProfile,
  type TestReading,
} from "@/entities/dataSource/dataSource.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

const collection = (systemId: string) =>
  `/api/v1/systems/${systemId}/data-sources`;

/** Lists the uploader kinds this instance can connect. @returns Validated source profiles. */
export async function fetchSourceProfiles(): Promise<SourceProfile[]> {
  return z
    .array(sourceProfileSchema)
    .parse(await sessionJsonRequest("/api/v1/data-source-profiles"));
}

/** Lists the data sources of a system with their connection state. @param systemId - System whose sources to list. @returns Validated data sources without secrets. */
export async function fetchDataSources(
  systemId: string,
): Promise<DataSource[]> {
  return z
    .array(dataSourceSchema)
    .parse(await sessionJsonRequest(collection(systemId)));
}

/** Creates a data source and returns its one-time configuration. @param systemId - System to feed. @param input - Name and profile. @returns The source with the configuration shown once. */
export async function createDataSource(
  systemId: string,
  input: { name: string; profile: string },
): Promise<IssuedDataSource> {
  return issuedDataSourceSchema.parse(
    await sessionJsonRequest(collection(systemId), {
      method: "POST",
      body: JSON.stringify(input),
    }),
  );
}

/** Renames a data source. @param systemId - Owning system. @param id - Data source to rename. @param name - New name. @returns The renamed data source. */
export async function renameDataSource(
  systemId: string,
  id: string,
  name: string,
): Promise<DataSource> {
  return dataSourceSchema.parse(
    await sessionJsonRequest(`${collection(systemId)}/${id}`, {
      method: "PUT",
      body: JSON.stringify({ name }),
    }),
  );
}

/** Replaces the key of a data source; the previous key stops working at once. @param systemId - Owning system. @param id - Data source whose key to replace. @returns The source with its new one-time configuration. */
export async function regenerateDataSourceCredential(
  systemId: string,
  id: string,
): Promise<IssuedDataSource> {
  return issuedDataSourceSchema.parse(
    await sessionJsonRequest(`${collection(systemId)}/${id}/credential`, {
      method: "POST",
    }),
  );
}

/** Deletes a data source and revokes its key; delivered readings are kept. @param systemId - Owning system. @param id - Data source to delete. @returns Completion after deletion. */
export async function deleteDataSource(
  systemId: string,
  id: string,
): Promise<void> {
  await sessionJsonRequest(`${collection(systemId)}/${id}`, {
    method: "DELETE",
  });
}

/** Stores one marked test reading for a data source. @param systemId - Owning system. @param id - Data source to test. @returns The stored test reading. */
export async function sendTestReading(
  systemId: string,
  id: string,
): Promise<TestReading> {
  return testReadingSchema.parse(
    await sessionJsonRequest(`${collection(systemId)}/${id}/test-reading`, {
      method: "POST",
    }),
  );
}

/** Removes the test readings of a system; real readings are untouched. @param systemId - System to clean. @returns Completion after removal. */
export async function deleteTestReadings(systemId: string): Promise<void> {
  await sessionJsonRequest(`/api/v1/systems/${systemId}/test-readings`, {
    method: "DELETE",
  });
}
