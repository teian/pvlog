import { z } from "zod";

const telemetryConfigSchema = z
  .object({
    enabled: z.boolean().default(false),
    endpoint: z.url().optional(),
    headers: z.record(z.string(), z.string()).default({}),
    serviceName: z.string().min(1).default("pvlog-ui"),
    serviceVersion: z.string().min(1).default("development"),
  })
  .refine((value) => !value.enabled || value.endpoint !== undefined, {
    message: "A telemetry endpoint is required when browser tracing is enabled",
    path: ["endpoint"],
  });

const runtimeConfigSchema = z.object({
  apiBaseUrl: z
    .string()
    .min(1)
    .refine((value) => value.startsWith("/") || URL.canParse(value), {
      message: "API base URL must be absolute or root-relative",
    }),
  telemetry: telemetryConfigSchema.default({
    enabled: false,
    headers: {},
    serviceName: "pvlog-ui",
    serviceVersion: "development",
  }),
});

/** Validated browser deployment configuration. */
export type RuntimeConfig = z.infer<typeof runtimeConfigSchema>;

/** Browser tracing configuration. */
export type TelemetryConfig = z.infer<typeof telemetryConfigSchema>;

/**
 * Loads deployment settings before the React tree is rendered.
 *
 * @returns Validated runtime configuration.
 */
export async function loadRuntimeConfig(): Promise<RuntimeConfig> {
  const response = await fetch("/runtime-config.json", {
    cache: "no-store",
    headers: { Accept: "application/json" },
  });

  if (!response.ok) {
    throw new Error(
      `Runtime configuration request failed with ${String(response.status)}`,
    );
  }

  return runtimeConfigSchema.parse(await response.json());
}
