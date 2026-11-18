<script setup lang="ts">
import type { TranscriptLine } from "@mm/ui/types";

/*
 * What a cluster target does, one topic per tab, each with the command and what it prints.
 * The transcripts follow the output shapes in docs/remote-daemon.md and docs/how-to/cluster.md
 * with the node list from the hero: keep them in step with the CLI.
 */
const topic = useClusterTopic();

interface Topic {
  value: string;
  label: string;
  title: string;
  body: string;
  /** Shown beside the copy when the feature has a limit the reader must not miss. */
  caution?: { title: string; body: string };
  link: { label: string; to: string };
  window: string;
  lines: (TranscriptLine | string)[];
}

const command = (text: string): TranscriptLine => ({ kind: "command", text });
const config = (text: string): TranscriptLine => ({ kind: "config", text });
const comment = (text: string): TranscriptLine => ({ kind: "comment", text });

const topics: Topic[] = [
  {
    value: "network",
    label: "Private network",
    title: "The nodes talk to each other over WireGuard.",
    body: "Setup gives every node an address on a mesh that `mmd` owns, and you configure nothing for it. One UDP port has to be open between the nodes, a node's private key never leaves it, and traffic between containers on different nodes travels inside the mesh.",
    link: { label: "The mesh between the nodes", to: "/docs/remote-daemon#the-mesh-between-the-nodes" },
    window: "mesh",
    lines: [
      command("mm -e production target mesh status"),
      "Target production: mesh between 3 node(s)",
      "",
      "  app-1  mm0 up  10.90.0.1  UDP 51820  mtu 1420  key 4kT9wq0aYx1B…",
      "    PEER             ENDPOINT                   HANDSHAKE      RECEIVED       SENT",
      "    app-2            203.0.113.12:51820         34 s ago         18 MiB     21 MiB",
      "    db-1             203.0.113.13:51820         12 s ago         62 KiB     66 KiB",
      "",
      "  app-2  mm0 up  10.90.0.2  UDP 51820  mtu 1420  key Zr81nQe0pL2c…",
      "    PEER             ENDPOINT                   HANDSHAKE      RECEIVED       SENT",
      "    app-1            203.0.113.11:51820         34 s ago         21 MiB     18 MiB",
      "    db-1             203.0.113.13:51820         9 s ago          31 KiB     31 KiB",
      "",
      "  db-1  mm0 up  10.90.0.3  UDP 51820  mtu 1420  key b0Hq7TtW3xNe…",
      "    PEER             ENDPOINT                   HANDSHAKE      RECEIVED       SENT",
      "    app-1            203.0.113.11:51820         12 s ago         66 KiB     62 KiB",
      "    app-2            203.0.113.12:51820         9 s ago          31 KiB     31 KiB",
      "",
      { text: "Every node has a recent handshake with every other.", tone: "ok" },
    ],
  },
  {
    value: "names",
    label: "Service names",
    title: "A container reaches `postgres` by name from any node.",
    body: "The networks in `mm.yaml` span the nodes, and each node's `mmd` answers the lookups of its own containers. A service that also runs on the asking node resolves to the replica there first. `tasks.<service>` lists every replica.",
    link: { label: "Service names across nodes", to: "/docs/remote-daemon#service-names-across-nodes" },
    window: "names",
    lines: [
      command("mm -e production service status"),
      comment("# below the service rows"),
      "  NAME                     NETWORK        RESOLVES TO",
      "  api                      backend        10.64.0.3 (app-1), 10.64.1.3 (app-2); app-1 and app-2 resolve to their own, db-1 to all",
      "  postgres                 backend        10.64.2.2 (db-1)",
      "  valkey                   backend        10.64.2.3 (db-1)",
      "  web                      frontend       10.64.16.2 (app-1), 10.64.17.2 (app-2); each node resolves to its own",
      "  worker                   backend        10.64.1.4 (app-2)",
    ],
  },
  {
    value: "deploys",
    label: "Rolling deploys",
    title: "A deploy replaces one node at a time.",
    body: "A node is done when its containers pass their health checks and every other node resolves them by name. Only then does the next node start. With `update: {order: start-first}` the new container starts beside the old one, so the service has no gap.",
    link: { label: "Rolling deploys", to: "/docs/remote-daemon#rolling-deploys-and-rollback" },
    window: "deploy",
    lines: [
      command("mm -e production deploy apply --plan 0a4ba0ae… --yes"),
      comment("# after the plan:"),
      "==> app-1: preparing",
      "==> app-1: app: starting the new web next to the old",
      "==> app-1: app",
      "==> app-2: preparing",
      "==> app-2: app: starting the new web next to the old",
      "==> app-2: app",
      "==> db-1: preparing",
      "==> db-1: app",
      "==> app-1: finishing",
      "==> app-2: finishing",
      "==> db-1: finishing",
      "Deployment 0a4ba0ae34677ecf6507139bc887347d251e8ca7ba19e9f006fc2f016be9f9ab on production [production]",
      { text: "  app-1            applied      stacks: app", tone: "ok" },
      { text: "  app-2            applied      stacks: app", tone: "ok" },
      { text: "  db-1             applied      stacks: app", tone: "ok" },
    ],
  },
  {
    value: "rollback",
    label: "Rollback",
    title: "A failed deploy puts every node back.",
    body: "When a container fails its health check, exits, or keeps restarting, the deploy stops. Every node it changed returns to what it ran before, the nodes it had not reached are never touched, and the report quotes the failing container's log. `mm deploy rollback` does the same for a deploy that finished.",
    link: {
      label: "Return to the previous deployment",
      to: "/docs/remote-daemon#return-to-the-previous-deployment",
    },
    window: "deploy",
    lines: [
      command("mm -e production deploy apply --plan 85549657… --yes"),
      comment("# after the plan:"),
      "==> app-1: preparing",
      "==> app-1: app",
      "==> app-2: preparing",
      "==> app-2: app",
      { text: "==> app-2: rolling back", tone: "warn" },
      { text: "==> app-1: rolling back", tone: "warn" },
      "Deployment 85549657b9e8dbab7d79adda5e1cbeb919803cfcab243882043abfbae414ebb4 on production [production]",
      "  app-1            rolled back  runs what it ran before",
      {
        text: "  app-2            FAILED       service 'web' of stack 'app': its health check fails; rolled back to what it ran before",
        tone: "bad",
      },
      "                   last log lines of the failed container:",
      "                     listening on :3000",
      "                     cannot reach the database at postgres:5432",
      "  db-1             not started  unchanged",
    ],
  },
  {
    value: "ingress",
    label: "Ingress",
    title: "An HTTPS route reaches the service on every node.",
    body: "A route is written as it is for one host. The proxy sends each request to a healthy container on its own node first and to another node when it has none, and a container that leaves the route finishes its requests before it stops.",
    link: {
      label: "Route traffic into the cluster",
      to: "/docs/how-to/cluster#route-traffic-into-the-cluster",
    },
    window: "ingress",
    lines: [
      comment("# .config/mm.yaml"),
      config("environments:"),
      config("  production:"),
      config("    target: production"),
      config("    domains: {shop: shop.example.com}"),
      config("stacks:"),
      config("  app:"),
      config("    files: [compose.yaml]"),
      config("    ingress: {domain: shop, service: web}"),
      "",
      command("mm -e production deploy plan"),
      comment("# among the plan's lines:"),
      "  ingress: app-1 takes the traffic on ports 80 and 443 and sends each request to a healthy container on the same node, else on any node; HTTPS uses a certificate from ACME (Let's Encrypt) for each HTTPS host",
      { text: "    https://shop.example.com -> app/web port 3000", tone: "ok" },
    ],
  },
  {
    value: "autoscale",
    label: "Autoscale",
    title: "A few readable lines scale a service with its load.",
    body: "`mmd` measures the service's containers on every node and adds or removes one when CPU or memory stays past a threshold. A new container gets traffic once it is healthy, and one that is removed drains first. Only `scale.max` is required.",
    link: { label: "Autoscale a service", to: "/docs/remote-daemon#autoscale-a-service" },
    window: "autoscale",
    lines: [
      comment("# .config/mm.yaml"),
      config("modules:"),
      config("  shop:"),
      config("    services:"),
      config("      web:"),
      config("        stack: app"),
      config("        placement: all"),
      config("        autoscale:"),
      config("          scale: {min: 1, max: 4}"),
      config("          metric:"),
      config("            type: cpu"),
      config("            up: {greater: 75%, for: 30s}"),
      config("            down: {less: 40%, for: 5m}"),
      "",
      command("mm -e production service status"),
      comment("# below the service rows"),
      "  autoscale web: 3 running, 3 wanted (1..4), decided on app-1",
      "    last change: 2 -> 3 at 2026-11-14T12:00:31Z: cpu 82% above 75% for 30s",
      "    cpu over the last 30 s: 81%",
      { text: "    now: cpu 81% above 75% for 10s of 30s", tone: "ok" },
    ],
  },
  {
    value: "volumes",
    label: "Shared volumes",
    title: "One line shares a volume between nodes.",
    body: "List a Compose volume under `shared-volumes` and every node that runs a service on it sees the same directory. One node keeps the files and serves them to the others over the mesh. The Compose file stays as it is.",
    caution: {
      title: "Use local volumes for databases and anything where performance matters.",
      body: "A shared volume is a network mount for files every replica needs, such as uploads. Creating a file takes 7 ms next to the storage node and 185 ms on a node 58 ms away, and while the storage node is down, reads and writes wait for it.",
    },
    link: { label: "Share a volume between nodes", to: "/docs/how-to/cluster#share-a-volume-between-nodes" },
    window: "storage",
    lines: [
      comment("# .config/mm.yaml"),
      config("stacks:"),
      config("  app:"),
      config("    files: [compose.yaml]"),
      config("    shared-volumes: [uploads]"),
      "",
      command("mm -e production target storage status"),
      "Target production: app-1 keeps shared volumes in /var/lib/mm/storage, 26.3 GiB free of 29.3 GiB",
      "",
      "  app/uploads              12.0 KiB in 3 files; mounted on app-1, app-2",
      "",
      "  app-1                    keeps the volumes and serves them on 10.90.0.1",
      "  app-2                    reaches app-1 in under 1 ms",
      "  db-1                     reaches app-1 in 58 ms",
      "",
      { text: "Every node reaches the storage node.", tone: "ok" },
    ],
  },
  {
    value: "logs",
    label: "One log",
    title: "One command reads a service's log from every node.",
    body: "`mm service logs web` merges the lines of every container of `web` on every node into one stream in time order, each line marked with its node and container. Leave out the service to follow the whole environment.",
    link: { label: "Read logs and status", to: "/docs/how-to/cluster#read-logs-and-status" },
    window: "logs",
    lines: [
      command("mm -e production service logs web --follow"),
      "app-1 web-1 | GET / 200 12ms",
      "app-2 web-1 | GET /assets/app.css 200 2ms",
      "app-1 web-1 | GET /api/projects 200 18ms request_id=4f1c",
      "app-2 web-1 | POST /api/deployments 201 142ms request_id=4f1d",
      {
        text: "app-2 web-1 | WARN slow query (812ms): SELECT * FROM deployments WHERE project_id = $1",
        tone: "warn",
      },
      "app-1 web-1 | GET /api/deployments/118 200 9ms request_id=4f1e",
      "app-2 web-1 | GET /healthz 200 1ms",
      "app-1 web-1 | GET /healthz 200 1ms",
    ],
  },
];

const tabs = topics.map(({ value, label }) => ({ value, label }));
</script>

<template>
  <section id="cluster" class="section cluster" data-tone="ok">
    <div class="cluster__backdrop" aria-hidden="true" />
    <div class="site-container">
      <header class="cluster__head">
        <h2 class="display display--lg" data-reveal>
          <span class="ink-metal">Everything between the nodes </span
          ><span class="ink-tone">is set up for you.</span>
        </h2>
        <p class="lede" data-reveal>
          <code class="cluster__cmd">mm target setup</code> puts the container engine, the
          <code>mmd</code> daemon, and a WireGuard mesh on every host. After that a cluster has four things to
          think about: nodes, services, networks, and volumes.
        </p>
      </header>

      <MmTabs v-model="topic" :tabs="tabs" label="What a cluster does" class="cluster__tabs" data-reveal>
        <template v-for="item in topics" :key="item.value" #[item.value]>
          <div class="cluster__panel">
            <div class="cluster__copy">
              <h3 class="display display--md"><MmCodeText :text="item.title" /></h3>
              <div class="cluster__text">
                <p><MmCodeText :text="item.body" /></p>
                <MmNotice
                  v-if="item.caution"
                  tone="warn"
                  :title="item.caution.title"
                  class="cluster__caution"
                >
                  {{ item.caution.body }}
                </MmNotice>
                <NuxtLink :to="item.link.to" class="cluster__link">{{ item.link.label }}</NuxtLink>
              </div>
            </div>
            <MmWindow class="cluster__window">
              <template #title>{{ item.window }}</template>
              <MmTranscript :lines="item.lines" :label="`Example: ${item.label}`" />
            </MmWindow>
          </div>
        </template>
      </MmTabs>

      <p class="cluster__limit" data-reveal>
        <MmTone tone="idle" />
        <span>
          There is no scheduler and no consensus store to run. A node that goes down comes back with its own
          services, and nothing moves to another node in the meantime.
          <NuxtLink to="/docs/how-to/cluster#when-a-node-goes-down">When a node goes down</NuxtLink>
        </span>
      </p>
    </div>
  </section>
</template>

<style scoped>
.cluster {
  overflow: clip;
}

.cluster__backdrop {
  position: absolute;
  inset: 0;
  z-index: -1;
  background:
    radial-gradient(ellipse 44% 42% at 74% 58%, rgb(var(--mm-accent-rgb) / 8%), transparent 72%),
    radial-gradient(rgb(232 246 214 / 8%) 1px, transparent 1.2px) 0 0 / 26px 26px;
  mask-image: linear-gradient(180deg, transparent 0%, #000 22%, #000 78%, transparent 100%);
}

.cluster__head {
  display: grid;
  gap: 1.4rem;
  justify-items: start;
  max-width: 54rem;
  margin-bottom: clamp(2rem, 4vw, 3rem);
}

.cluster__head .lede {
  max-width: 40rem;
}

.cluster__head code {
  font-size: 0.92em;
  color: var(--mm-ink);
}

.cluster__cmd {
  padding: 0.08em 0.45em;
  white-space: nowrap;
  background: rgb(var(--mm-accent-rgb) / 9%);
  border: 1px solid rgb(var(--mm-accent-rgb) / 22%);
  border-radius: 5px;
}

.cluster__head .cluster__cmd {
  color: var(--mm-accent);
}

.cluster__tabs :deep(.mm-tabs__panel) {
  margin-top: clamp(1.5rem, 3vw, 2.25rem);
}

/* The copy sits above a window as wide as the page, so terminal output keeps its columns. */
.cluster__panel {
  display: grid;
  gap: clamp(1.5rem, 3vw, 2.25rem);
}

.cluster__copy {
  display: grid;
  grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr);
  gap: clamp(1.25rem, 4vw, 4rem);
  align-items: start;
}

.cluster__copy h3 {
  max-width: 26rem;
  color: var(--mm-ink);
}

.cluster__copy h3 :deep(code) {
  font-size: 0.86em;
  font-weight: 600;
  color: var(--mm-accent);
}

.cluster__text {
  display: grid;
  gap: 1rem;
  justify-items: start;
}

.cluster__text p {
  margin: 0;
  max-width: 40rem;
  line-height: 1.6;
  color: var(--mm-muted);
}

/* A key or command named in the text stays on one line. */
.cluster__text p :deep(code) {
  font-size: 0.88em;
  color: var(--mm-ink-soft);
  white-space: nowrap;
}

.cluster__caution {
  font-size: 0.9063rem;
  line-height: 1.5;
}

.cluster__link {
  display: inline-flex;
  gap: 0.4rem;
  align-items: center;
  font-size: 0.9063rem;
  font-weight: 500;
  color: var(--mm-ink-soft);
  text-decoration: none;
  transition: color 0.2s var(--mm-ease);
}

.cluster__link::after {
  content: "→";
  transition: transform 0.2s var(--mm-ease);
}

.cluster__link:hover {
  color: var(--mm-accent);
}

.cluster__link:hover::after {
  transform: translateX(3px);
}

.cluster__window {
  --mm-window-bg: #080a09;

  box-shadow:
    0 0 0 1px rgb(232 246 214 / 14%),
    inset 0 1px 0 rgb(255 255 255 / 8%),
    0 50px 100px -50px rgb(0 0 0 / 90%),
    0 30px 80px -44px rgb(var(--mm-accent-rgb) / 20%);
}

/* Tall enough for the longest transcript, so the page keeps its height from tab to tab. */
.cluster__window :deep(.mm-transcript) {
  --mm-transcript-pad: 1.1rem 1.4rem 1.25rem;
  --mm-transcript-size: 0.8125rem;

  min-height: 27.5rem;
}

.cluster__limit {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 0.75rem;
  align-items: start;
  max-width: 50rem;
  margin: clamp(2rem, 4vw, 3rem) 0 0;
  font-size: 0.9375rem;
  line-height: 1.55;
  color: var(--mm-muted);
}

.cluster__limit .mm-tone {
  margin-top: 0.42em;
  font-size: 0.7rem;
}

.cluster__limit a {
  color: var(--mm-ink-soft);
  text-decoration: underline;
  text-decoration-color: var(--mm-line-strong);
  text-underline-offset: 0.2em;
  white-space: nowrap;
  transition:
    color 0.2s var(--mm-ease),
    text-decoration-color 0.2s var(--mm-ease);
}

.cluster__limit a:hover {
  color: var(--mm-accent);
  text-decoration-color: currentcolor;
}

@media (max-width: 860px) {
  .cluster__copy {
    grid-template-columns: minmax(0, 1fr);
  }

  .cluster__window :deep(.mm-transcript) {
    --mm-transcript-pad: 1rem 1.1rem 1.1rem;
    --mm-transcript-size: 0.75rem;

    min-height: 0;
  }
}
</style>
