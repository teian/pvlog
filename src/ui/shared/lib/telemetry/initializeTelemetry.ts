import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-http";
import { registerInstrumentations } from "@opentelemetry/instrumentation";
import { DocumentLoadInstrumentation } from "@opentelemetry/instrumentation-document-load";
import { FetchInstrumentation } from "@opentelemetry/instrumentation-fetch";
import { resourceFromAttributes } from "@opentelemetry/resources";
import {
  BatchSpanProcessor,
  WebTracerProvider,
} from "@opentelemetry/sdk-trace-web";
import {
  ATTR_SERVICE_NAME,
  ATTR_SERVICE_VERSION,
} from "@opentelemetry/semantic-conventions";

import type { TelemetryConfig } from "@/shared/config";

let provider: WebTracerProvider | undefined;

/**
 * Initializes browser tracing once when runtime configuration enables it.
 *
 * @param config - Validated browser telemetry settings.
 * @returns The registered provider, or undefined when tracing is disabled.
 */
export function initializeTelemetry(config: TelemetryConfig) {
  if (!config.enabled || config.endpoint === undefined) {
    return undefined;
  }
  if (provider !== undefined) {
    return provider;
  }

  const exporter = new OTLPTraceExporter({
    url: config.endpoint,
    headers: config.headers,
  });
  provider = new WebTracerProvider({
    resource: resourceFromAttributes({
      [ATTR_SERVICE_NAME]: config.serviceName,
      [ATTR_SERVICE_VERSION]: config.serviceVersion,
    }),
    spanProcessors: [new BatchSpanProcessor(exporter)],
  });
  provider.register();

  registerInstrumentations({
    instrumentations: [
      new DocumentLoadInstrumentation(),
      new FetchInstrumentation({ ignoreUrls: [config.endpoint] }),
    ],
  });

  return provider;
}
