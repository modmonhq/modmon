import { fileURLToPath } from "node:url";
import { defineCollection, defineContentConfig } from "@nuxt/content";

// The documentation is plain Markdown in the repository's docs/ directory, so it stays
// readable on GitHub. This site only renders it.
export default defineContentConfig({
  collections: {
    docs: defineCollection({
      type: "page",
      source: {
        cwd: fileURLToPath(new URL("../docs", import.meta.url)),
        include: "**/*.md",
        prefix: "/docs",
      },
    }),
  },
});
