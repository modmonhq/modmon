import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";

// The UI kit is shared with the site and consumed from source. It has no node_modules of
// its own, so its `vue` and `reka-ui` imports are deduped onto this package's copies.
const kit = fileURLToPath(new URL("../../packages/ui/src", import.meta.url));

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: { "@mm/ui": kit },
    dedupe: ["vue", "reka-ui"],
  },
  server: { fs: { allow: [".", kit] } },
  build: {
    sourcemap: false,
    assetsInlineLimit: 0,
    rollupOptions: {
      output: {
        entryFileNames: "assets/app-[hash].js",
        chunkFileNames: "assets/chunk-[hash].js",
        assetFileNames: "assets/[name]-[hash][extname]",
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: "./src/test-setup.ts",
    css: true,
    exclude: ["tests/**", "node_modules/**", "dist/**"],
    coverage: {
      provider: "v8",
      include: ["src/**/*.{ts,vue}"],
      exclude: [
        "src/**/*.test.ts",
        "src/test-*.ts",
        "src/**/*.d.ts",
        "src/api.generated.ts",
        "src/api.validator.generated.js",
      ],
      reporter: ["text", "json-summary", "html", "lcov", "cobertura"],
      reportsDirectory: "coverage",
      thresholds: {
        statements: 100,
        branches: 100,
        functions: 100,
        lines: 100,
      },
    },
  },
});
