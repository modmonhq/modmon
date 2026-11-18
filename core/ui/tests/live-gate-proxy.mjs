import { createServer as createHttpsServer } from "node:https";
import { request as httpRequest } from "node:http";
import { readFileSync } from "node:fs";

const key = readFileSync(process.env.MM_GATE_TLS_KEY);
const cert = readFileSync(process.env.MM_GATE_TLS_CERT);
const ingressToken = readFileSync(process.env.MM_GATE_INGRESS_TOKEN, "utf8");
const controlHost = process.env.MM_GATE_CONTROL_HOST;
const controlPort = Number(process.env.MM_GATE_CONTROL_PORT ?? "43128");
const capHost = process.env.MM_GATE_CAP_HOST;
const capPort = Number(process.env.MM_GATE_CAP_PORT ?? "43127");
const policyId = process.env.MM_GATE_POLICY_ID;

if (!controlHost || !capHost || !policyId) {
  throw new Error("MM_GATE_CONTROL_HOST, MM_GATE_CAP_HOST, and MM_GATE_POLICY_ID are required");
}

createHttpsServer({ key, cert }, (incoming, outgoing) => {
  const authority = incoming.headers.host ?? "";
  const capRequest = authority.split(":", 1)[0] === "cap.mm.test";
  if (process.env.MM_GATE_PROXY_LOG === "1") {
    const cookieNames = (incoming.headers.cookie ?? "")
      .split(";")
      .map((cookie) => cookie.trim().split("=", 1)[0])
      .filter(Boolean);
    console.log(incoming.method, authority, incoming.url, {
      origin: incoming.headers.origin,
      cookieNames,
    });
  }
  const headers = { ...incoming.headers };
  delete headers["x-mm-ingress-auth"];
  delete headers["x-forwarded-for"];
  delete headers["x-forwarded-host"];
  delete headers["x-forwarded-proto"];
  delete headers["x-forwarded-uri"];
  headers.host = authority;
  headers["x-forwarded-for"] = incoming.socket.remoteAddress ?? "unknown";
  if (!capRequest) {
    headers["x-mm-ingress-auth"] = ingressToken;
    headers["x-forwarded-host"] = authority;
    headers["x-forwarded-proto"] = "https";
    headers["x-forwarded-uri"] = incoming.url ?? "/";
  }
  const proxy = (responseCookies = []) => {
    const upstream = httpRequest(
      {
        host: capRequest ? capHost : controlHost,
        port: capRequest ? capPort : controlPort,
        method: incoming.method,
        path: incoming.url,
        headers,
      },
      (response) => {
        if (process.env.MM_GATE_PROXY_LOG === "1") {
          console.log("upstream", response.statusCode, incoming.method, authority, incoming.url);
        }
        const responseHeaders = { ...response.headers };
        if (responseCookies.length > 0) responseHeaders["set-cookie"] = responseCookies;
        outgoing.writeHead(response.statusCode ?? 502, responseHeaders);
        response.pipe(outgoing);
      },
    );
    upstream.on("error", () => {
      outgoing.writeHead(502, { "content-type": "text/plain" });
      outgoing.end("upstream unavailable");
    });
    incoming.pipe(upstream);
  };

  const gateRequest = (incoming.url ?? "").startsWith("/_mm/gate/");
  if (capRequest || gateRequest) {
    proxy();
    return;
  }

  const authorization = httpRequest(
    {
      host: controlHost,
      port: controlPort,
      method: "GET",
      path: `/_mm/gate/forward/${policyId}`,
      headers,
    },
    (response) => {
      const status = response.statusCode ?? 502;
      if (status >= 200 && status < 300) {
        const principal = response.headers["x-mm-gate-principal"];
        if (principal) headers["x-mm-gate-principal"] = principal;
        response.resume();
        proxy(response.headers["set-cookie"] ?? []);
        return;
      }
      outgoing.writeHead(status, response.headers);
      response.pipe(outgoing);
    },
  );
  authorization.on("error", () => {
    outgoing.writeHead(502, { "content-type": "text/plain" });
    outgoing.end("authorization unavailable");
  });
  authorization.end();
}).listen(443, "0.0.0.0");
