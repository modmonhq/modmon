// Sidebar order for the documentation. Entries are paths under docs/ without the extension;
// "index" is the overview at /docs. Labels come from each page's `title` frontmatter.
// Every file in docs/ should appear exactly once: a page missing from this list is still
// served, but nothing links to it from the sidebar.
export interface DocsNavGroup {
  title: string;
  pages: string[];
}

export const docsNav: DocsNavGroup[] = [
  {
    title: "Start",
    pages: [
      "index",
      "start/install",
      "getting-started",
      "tutorials/first-local-app",
      "onboarding",
      "tutorials/node-fullstack",
      "tutorials/dotnet-web-api",
      "tutorials/laravel-rust",
    ],
  },
  {
    title: "Clusters and remote hosts",
    pages: [
      "how-to/cluster",
      "how-to/cloudflare-tunnel",
      "how-to/change-a-variable",
      "how-to/upgrade",
      "how-to/managed-secret-store",
      "how-to/from-docker-swarm",
      "concepts/targets-and-runtimes",
      "remote-daemon",
      "multi-project-hosts",
      "remote-access",
      "hardening",
    ],
  },
  {
    title: "Local development",
    pages: ["local-services", "local-https", "development-mounts", "local-dashboard"],
  },
  {
    title: "Project configuration",
    pages: [
      "concepts/mental-model",
      "concepts/configuration-ownership",
      "reference/configuration",
      "concepts/environments-and-secrets",
      "concepts/plans-and-apply",
      "concepts/builds-and-deployments",
    ],
  },
  {
    title: "Run and troubleshoot",
    pages: [
      "terminal-ui",
      "runtime-observability",
      "jobs-and-schedules",
      "config-files",
      "how-to/deploy-production",
      "how-to/production-checklist",
      "help/troubleshooting",
      "help/faq",
    ],
  },
  {
    title: "Secrets",
    pages: ["secret-delivery", "secret-providers", "openbao", "vault"],
  },
  { title: "Access", pages: ["access-gating"] },
  { title: "Coding tools", pages: ["mcp"] },
  {
    title: "Reference",
    pages: [
      "reference/cli",
      "reference/environment-variables",
      "reference/errors-and-exit-codes",
      "reference/compatibility",
      "reference/capability-matrix",
      "reference/known-issues",
      "architecture/security-and-trust",
    ],
  },
  {
    title: "Contribute",
    pages: [
      "project/contributing",
      "project/security",
      "contribute/releases",
      "contribute/remote-protocol",
      "project/license",
    ],
  },
];

/** The site route for a page listed above. */
export function docsRoute(page: string): string {
  return page === "index" ? "/docs" : `/docs/${page}`;
}
