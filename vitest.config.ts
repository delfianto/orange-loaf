import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

// Deliberately separate from vite.config.ts: that one carries Tauri dev-server
// settings (fixed port, HMR host, watch ignores) that have no bearing on
// running pure-function unit tests.
export default defineConfig({
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
