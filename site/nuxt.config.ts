import { execFileSync } from "node:child_process";
import { copyFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import docsCodeTheme from "./app/docs.code-theme";

// The UI kit is shared with the local dashboard in core/ui and consumed from source.
const ui = fileURLToPath(new URL("../packages/ui/src", import.meta.url));
const installer = fileURLToPath(new URL("../packaging/release/install.sh", import.meta.url));

function latestRelease(): string {
  try {
    return execFileSync(
      "git",
      ["describe", "--tags", "--abbrev=0", "--match", "v[0-9]*", "--exclude", "*-*"],
      {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "ignore"],
      },
    ).trim();
  } catch {
    return "";
  }
}

export default defineNuxtConfig({
  compatibilityDate: "2026-11-01",
  devtools: { enabled: false },
  telemetry: false,
  modules: ["@nuxt/content"],
  alias: { "@mm/ui": ui },
  runtimeConfig: { public: { release: latestRelease() } },
  components: [
    { path: "~/components", pathPrefix: false },
    { path: `${ui}/components`, pathPrefix: false },
  ],
  css: ["@mm/ui/tokens.css", "@mm/ui/base.css", "~/assets/css/fonts.css", "~/assets/css/site.css"],
  typescript: {
    tsConfig: {
      // The kit has no node_modules of its own; resolve its imports from this package.
      compilerOptions: {
        paths: { vue: ["../node_modules/vue"], "reka-ui": ["../node_modules/reka-ui"] },
      },
    },
  },
  vite: {
    resolve: { dedupe: ["vue", "reka-ui"] },
    server: { fs: { allow: [".."] } },
  },
  content: {
    experimental: { sqliteConnector: "native" },
    build: {
      markdown: {
        // Shiki accepts this TextMate-style theme at runtime; its type also asks for a
        // legacy `settings` list that would only duplicate `tokenColors`.
        highlight: { theme: { default: docsCodeTheme as never }, langs: ["sh", "yaml", "json", "dotenv"] },
      },
    },
  },
  app: {
    // Route changes swap the page with a short fade; persistent layouts stay put.
    pageTransition: { name: "page", mode: "out-in" },
    layoutTransition: { name: "layout", mode: "out-in" },
    head: {
      // Pages set their own; this covers the fallback shells the generator writes.
      title: "mm",
      htmlAttrs: { lang: "en" },
      meta: [
        { name: "color-scheme", content: "dark" },
        { name: "theme-color", content: "#070908" },
      ],
      link: [
        { rel: "icon", type: "image/svg+xml", href: "/favicon.svg" },
        { rel: "icon", sizes: "48x48", href: "/favicon.ico" },
        { rel: "apple-touch-icon", href: "/apple-touch-icon.png" },
      ],
    },
  },
  hooks: {
    async "nitro:build:public-assets"(nitro) {
      await copyFile(installer, join(nitro.options.output.publicDir, "install.sh"));
    },
  },
  nitro: {
    prerender: { crawlLinks: true, routes: ["/"], failOnError: true },
    // Images sit beside the Markdown so they also show on GitHub. A page at /docs/<name>
    // refers to them as ./images/…, which a browser resolves to /docs/images/….
    publicAssets: [
      { dir: fileURLToPath(new URL("../docs/images", import.meta.url)), baseURL: "/docs/images" },
    ],
  },
});
