import path from "node:path";
import { fileURLToPath } from "node:url";

import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const repositoryRoot = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(repositoryRoot, "src/ui"),
    },
  },
  test: {
    environment: "jsdom",
    include: ["tests/ui/**/*.{test,spec}.{ts,tsx}"],
    setupFiles: ["tests/ui/setup.ts"],
    clearMocks: true,
    restoreMocks: true,
    unstubEnvs: true,
    unstubGlobals: true,
  },
});
