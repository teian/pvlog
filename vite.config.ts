import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv, type Plugin } from "vite";

const REPOSITORY_ROOT = path.dirname(fileURLToPath(import.meta.url));
const OPENAPI_PATH = path.resolve(REPOSITORY_ROOT, "openapi/pvlog-v1.yaml");

function openApiAssetPlugin(): Plugin {
  const contract = readFileSync(OPENAPI_PATH, "utf8");

  return {
    name: "pvlog-openapi-asset",
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        if (request.url?.split("?", 1)[0] !== "/openapi/pvlog-v1.yaml") {
          next();
          return;
        }

        response.statusCode = 200;
        response.setHeader("Content-Type", "application/yaml; charset=utf-8");
        response.setHeader("Cache-Control", "no-cache");
        response.end(readFileSync(OPENAPI_PATH, "utf8"));
      });
    },
    generateBundle() {
      this.emitFile({
        type: "asset",
        fileName: "openapi/pvlog-v1.yaml",
        source: contract,
      });
    },
  };
}

function parseOtlpHeaders(value: string | undefined) {
  if (value === undefined || value.trim() === "") {
    return {};
  }

  const headers: Record<string, string> = {};
  for (const item of value.split(",")) {
    const [name, ...valueParts] = item.split("=");
    if (name !== undefined && name.trim() !== "") {
      headers[name.trim()] = valueParts.join("=").trim();
    }
  }
  return headers;
}

function runtimeConfigPlugin(environment: Record<string, string>): Plugin {
  const runtimeConfig = JSON.stringify({
    apiBaseUrl: environment.VITE_API_BASE_URL ?? "/api/v1",
    telemetry: {
      enabled: environment.VITE_OTEL_ENABLED === "true",
      endpoint:
        environment.VITE_OTEL_EXPORTER_OTLP_TRACES_ENDPOINT ??
        environment.VITE_OTEL_EXPORTER_OTLP_ENDPOINT,
      serviceName: environment.VITE_OTEL_SERVICE_NAME ?? "pvlog-ui",
      serviceVersion: environment.VITE_APP_VERSION ?? "development",
      headers: parseOtlpHeaders(environment.VITE_OTEL_EXPORTER_OTLP_HEADERS),
    },
  });

  return {
    name: "pvlog-runtime-config",
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        if (request.url?.split("?", 1)[0] !== "/runtime-config.json") {
          next();
          return;
        }

        response.statusCode = 200;
        response.setHeader("Content-Type", "application/json; charset=utf-8");
        response.setHeader("Cache-Control", "no-store");
        response.end(runtimeConfig);
      });
    },
  };
}

export default defineConfig(({ mode }) => {
  const environment = loadEnv(mode, REPOSITORY_ROOT, "");

  return {
    root: path.resolve(REPOSITORY_ROOT, "src/ui"),
    plugins: [
      openApiAssetPlugin(),
      runtimeConfigPlugin(environment),
      react(),
      tailwindcss(),
    ],
    resolve: {
      alias: {
        "@": path.resolve(REPOSITORY_ROOT, "src/ui"),
      },
    },
    server: {
      proxy: {
        "/api/v1": {
          target:
            environment.VITE_DEV_API_TARGET ?? "http://127.0.0.1:18087",
          changeOrigin: true,
        },
      },
    },
    build: {
      outDir: path.resolve(REPOSITORY_ROOT, "dist/ui"),
      emptyOutDir: true,
    },
  };
});
