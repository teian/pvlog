import { z } from "zod";

const id = z.uuid();

/** Kind of uploader a data source represents. */
export const sourceProfileSchema = z.object({
  id: z.string().min(1),
  displayName: z.string().min(1),
  issuesCredential: z.boolean(),
  snippetKinds: z.array(z.string()),
});

/** Whether a data source still delivers readings. */
export const connectionStateSchema = z.enum([
  "never_connected",
  "receiving",
  "stale",
  "disconnected",
]);

/** One uploader or import feeding a system, without any secret. */
export const dataSourceSchema = z.object({
  id,
  systemId: id,
  name: z.string(),
  profile: z.string(),
  state: connectionStateSchema,
  serverUrl: z.string(),
  credentialId: id.nullable().default(null),
  lastReadingAtEpochMillis: z.number().int().nullable().default(null),
  lastReceivedAtEpochMillis: z.number().int().nullable().default(null),
  readingsLastDay: z.number().int().nonnegative(),
  createdAtEpochMillis: z.number().int(),
  updatedAtEpochMillis: z.number().int(),
});

/** One ready-to-paste piece of uploader configuration. */
export const configurationSnippetSchema = z.object({
  id: z.string(),
  title: z.string(),
  language: z.string(),
  content: z.string(),
});

/** Uploader configuration with the cleartext key; exists only in memory. */
export const dataSourceConfigurationSchema = z.object({
  serverUrl: z.string(),
  systemId: id,
  apiKey: z.string().min(1),
  snippets: z.array(configurationSnippetSchema),
});

/** A data source together with the configuration issued for it. */
export const issuedDataSourceSchema = z.object({
  dataSource: dataSourceSchema,
  configuration: dataSourceConfigurationSchema.nullable().default(null),
});

/** A reading that only proves a data source can deliver. */
export const testReadingSchema = z.object({
  id,
  systemId: id,
  dataSourceId: id.nullable().default(null),
  measuredAtEpochMillis: z.number().int(),
  receivedAtEpochMillis: z.number().int(),
  test: z.literal(true),
});

export type SourceProfile = z.infer<typeof sourceProfileSchema>;
export type ConnectionState = z.infer<typeof connectionStateSchema>;
export type DataSource = z.infer<typeof dataSourceSchema>;
export type ConfigurationSnippet = z.infer<typeof configurationSnippetSchema>;
export type DataSourceConfiguration = z.infer<
  typeof dataSourceConfigurationSchema
>;
export type IssuedDataSource = z.infer<typeof issuedDataSourceSchema>;
export type TestReading = z.infer<typeof testReadingSchema>;
