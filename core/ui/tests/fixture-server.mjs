import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join } from "node:path";

const root = new URL("../dist/", import.meta.url);
const csp = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; font-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";
const dashboard = {
  api_version: "1",
  generated_at_unix_ms: 1700000000000,
  scope: { kind: "installation" },
  installation: { installed: true, stack_version: "1.0.0", runtime: "docker-compose", route: "https://mm.test", components: [{ kind: "local_ingress", version: "3", running: true, healthy: true }, { kind: "mailpit", version: "1.21", running: true, healthy: false }], diagnostics: [{ code: "services.mailpit.unhealthy", severity: "warning", summary: "Mailpit is running but not answering health checks", suggested_action: "Inspect it with `mm dev services status`." }] },
  projects: [{ project_id: "219c527b-6242-7b43-bfd4-bd60e27c1de3", name: "example-with-a-long-project-name", slug: "example", dashboard_url: "https://mm.example.test", environment: "local" }],
};

createServer(async (request, response) => {
  response.setHeader("content-security-policy", csp);
  response.setHeader("x-content-type-options", "nosniff");
  if (request.url === "/api/v1/dashboard") {
    response.setHeader("content-type", "application/json");
    response.end(JSON.stringify(dashboard));
    return;
  }
  if (request.url === "/api/v1/events") {
    response.setHeader("content-type", "text/event-stream");
    response.end('event: connected\ndata: {"type":"connected","api_version":"1","sequence":1}\n\n');
    return;
  }
  const requested = request.url?.split("?", 1)[0] ?? "/";
  const safe = requested === "/" || /^\/assets\/[a-zA-Z0-9._-]+$/.test(requested);
  const file = safe && requested !== "/" ? requested.slice(1) : "index.html";
  try {
    const bytes = await readFile(join(root.pathname, file));
    response.setHeader("content-type", extname(file) === ".js" ? "text/javascript" : extname(file) === ".css" ? "text/css" : extname(file) === ".woff2" ? "font/woff2" : extname(file) === ".svg" ? "image/svg+xml" : "text/html");
    response.end(bytes);
  } catch {
    response.statusCode = 404;
    response.end("not found");
  }
}).listen(42832, "127.0.0.1");
