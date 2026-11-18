import type { Hint, Tone } from "@mm/ui/types";

/*
 * State for the terminal interface shown on the lander. It mirrors the real interface in
 * core/src/presentation/tui: the same views, sidebar groups, digit keys, status wording,
 * and key hints. Update it when the TUI changes.
 *
 * The demo opens on `production`, a cluster target of three nodes, so the first thing a
 * visitor sees is a deploy rolling through the nodes and one log merged from them.
 */

export type TabId =
  | "overview"
  | "environments"
  | "services"
  | "ingress"
  | "variables"
  | "mounts"
  | "diagnostics"
  | "tls"
  | "access"
  | "gates"
  | "providers"
  | "jobs"
  | "guided";

export const TAB_TITLES: Record<TabId, string> = {
  overview: "Overview",
  environments: "Environments",
  services: "Services",
  ingress: "Ingress",
  variables: "Variables",
  mounts: "Mounts",
  diagnostics: "Diagnostics",
  tls: "TLS",
  access: "Access",
  gates: "Gates",
  providers: "Providers",
  jobs: "Jobs",
  guided: "Guided",
};

export const TAB_GROUPS: { title: string; tabs: TabId[] }[] = [
  {
    title: "Workspace",
    tabs: ["overview", "environments", "services", "ingress", "variables", "mounts"],
  },
  { title: "Health", tabs: ["diagnostics", "tls"] },
  { title: "Security", tabs: ["access", "gates", "providers"] },
  { title: "Automation", tabs: ["jobs"] },
  { title: "Setup", tabs: ["guided"] },
];

export const TABS: TabId[] = TAB_GROUPS.flatMap((group) => group.tabs);
const DIGITS = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];

export function tabDigit(tab: TabId): string | undefined {
  return DIGITS[TABS.indexOf(tab)];
}

export interface DemoService {
  name: string;
  module: string;
  stack: string;
  desired: number;
  running: number;
  present: boolean;
  restarts: number;
  /** Whether its Compose file gives it a health check: a container reads `healthy`, not `running`. */
  checked: boolean;
  /** On a cluster target: the nodes that each run one container of the service. */
  nodes?: string[];
  /** The node whose new container a rollout or restart just started; it is not healthy yet. */
  replacing?: string;
  /** Nodes whose container was replaced a moment ago. */
  fresh?: string[];
}

/** One host of a cluster target. */
export interface DemoNode {
  name: string;
  host: string;
  mesh: string;
}

export interface DemoRoute {
  service: string;
  stack: string;
  url?: string;
  port?: number;
  source?: string;
  reason?: string;
}

export interface DemoVariable {
  service: string;
  stack: string;
  first: boolean;
  name: string;
  status: "resolved" | "defaulted" | "unset";
  source: "config" | "secret" | "local override" | "template default" | "missing";
  value?: string;
  sensitive?: boolean;
  key?: string;
  line: number;
}

/**
 * One row of the Services view: a service, one of its containers on this machine, or on a
 * cluster one of its nodes. Node rows are muted and can be selected, as in the TUI.
 */
export type ServiceRow =
  | { kind: "service"; service: DemoService; expanded: boolean }
  | {
      kind: "container";
      service: DemoService;
      name: string;
      state: "running" | "exited";
      last: boolean;
    }
  | {
      kind: "node";
      service: DemoService;
      node: string;
      /** The container's short name, as `mm service status` prints it. */
      container: string;
      /** Its health check's verdict, or `running` without one. */
      health: "healthy" | "running" | "starting";
      /** The engine's status words. */
      status: string;
      fresh: boolean;
      last: boolean;
    };

export interface DemoEnvironment {
  name: string;
  target: string;
  /** The hosts its `ingress:` routes answer on, by service; this machine makes its own. */
  domains?: Record<string, string>;
  /** `host:port` for a remote target that names one host. */
  host?: string;
  /** The hosts of a cluster target. */
  nodes?: DemoNode[];
  runtime: string;
  deployFrom?: string;
  isDefault?: boolean;
}

export interface DemoMember {
  name: string;
  /** Level per environment name; `*` is every remote environment. */
  access: Record<string, string>;
  keys: number;
  machine?: boolean;
  you?: boolean;
}

export interface DemoLine {
  text: string;
  stderr?: boolean;
  tone?: Tone;
  time?: string;
  /** Where the line came from, when the stream merges several nodes. */
  node?: string;
  container?: string;
}

export interface MenuItem {
  key: string;
  label: string;
  detail: string;
  disabled?: string;
}

export type DemoPanel =
  | {
      kind: "logs";
      label: string;
      /** The node of the one container the view follows, as `on app-2`. */
      where?: string;
      /** The nodes whose containers the stream merges; empty off a cluster. */
      nodes: string[];
      lines: DemoLine[];
      following: boolean;
      unseen: number;
      times: boolean;
    }
  | { kind: "run"; label: string; command: string; lines: DemoLine[]; exit?: number };

/** One node's row while a plan is applied, as the review lists it. */
export interface DemoReviewNode {
  node: string;
  stage: "waiting" | "working" | "done";
  /** The step it is on, in the words `mm deploy apply` prints. */
  step?: string;
  /** The stacks it applied so far. */
  stacks: string[];
}

/** The review of a plan, and where its apply is: shown full width, like a log. */
export interface DemoReview {
  remote: boolean;
  stage: "reviewing" | "applying" | "applied";
  /** `app on target 'production' (app-1, app-2, db-1)` */
  head: string;
  routes: number;
  plan: string;
  /** The plan as `mm deploy plan` prints it. */
  lines: string[];
  nodes: DemoReviewNode[];
  summary?: string;
  /** The first line of the plan in view. */
  scroll: number;
  /** Hidden with `esc` while the apply goes on; `u` shows it again. */
  shown: boolean;
}

export const LEVELS = ["none", "view", "operate", "deploy", "admin"] as const;

export interface DemoEditor {
  stage: "form" | "review" | "done";
  focus: number;
  name: string;
  key: string;
  /** Level per remote environment, in the order the form lists them. */
  levels: Record<string, string>;
  error?: string;
}

const PROJECT = "acme";

/** The cluster the demo opens on: the node list from the configuration next to the terminal. */
const NODES: DemoNode[] = [
  { name: "app-1", host: "203.0.113.11", mesh: "10.90.0.1" },
  { name: "app-2", host: "203.0.113.12", mesh: "10.90.0.2" },
  { name: "db-1", host: "203.0.113.13", mesh: "10.90.0.3" },
];

const ENVIRONMENTS: DemoEnvironment[] = [
  { name: "development", target: "local", runtime: "docker compose", isDefault: true },
  {
    name: "staging",
    target: "staging",
    domains: { web: "staging.acme.example.com", api: "api.staging.acme.example.com" },
    host: "staging.acme.example:22",
    runtime: "docker compose",
    deployFrom: "main",
  },
  {
    name: "production",
    target: "production",
    domains: { web: "acme.example.com", api: "api.acme.example.com" },
    nodes: NODES,
    runtime: "docker compose",
    deployFrom: "release",
  },
];

/** The environment the demo opens on and returns to. */
const HOME = "production";
/** What each remote environment runs when a tour begins, and the commit a deploy rolls out. */
const DEPLOYED: Record<string, string> = { staging: "9f2c1e7ab4d0", production: "9d1b6a4c2e80" };
const NEXT_COMMIT = "3f2c9a1e5b7d";
const NEXT_COMMIT_FULL = "3f2c9a1e5b7d40c8e2a96f1b3d57e0c9a4b8f612";
const PLAN_HASH = "8a41c0de91f2b7e04c6d29a3f5e18b7c0d4a9e62f13b85c7a0e9d4f26b1c8a53";
const NEXT_PLAN = PLAN_HASH.slice(0, 12);
/** The plan of the stack on this machine. */
const LOCAL_PLAN_HASH = "5c0e7b21d94a3f68e1b2c7d40a9f5e83b6d21c4f07e9a58b3c6d1e2f40a7b98c";
/** The services whose image the next commit changes. */
const CHANGED = ["web", "api"];

const TEAM_SEED: DemoMember[] = [
  { name: "alice", access: { staging: "deploy", production: "deploy +exec[api]" }, keys: 2, you: true },
  { name: "bob", access: { "*": "view" }, keys: 1 },
  { name: "ci", access: { staging: "deploy", production: "deploy" }, keys: 1, machine: true },
];

const DEMO_KEY = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGJ5dGVz… carol@laptop";

/** What a service prints while it runs; the log view cycles through these. */
const LOG_POOL: DemoLine[] = [
  { text: "GET /healthz 200 1ms" },
  { text: "GET /api/projects 200 18ms request_id=4f1c" },
  { text: "POST /api/deployments 201 142ms request_id=4f1d" },
  {
    text: "WARN slow query (812ms): SELECT * FROM deployments WHERE project_id = $1",
    stderr: true,
    tone: "warn",
  },
  { text: "GET /api/deployments/118 200 9ms request_id=4f1e" },
  { text: "GET /healthz 200 1ms" },
  { text: "ERROR upstream worker timed out after 30s request_id=4f1f", stderr: true, tone: "bad" },
  { text: "POST /api/deployments 502 30004ms request_id=4f1f" },
  { text: "Worker reconnected; draining 3 queued jobs" },
  { text: "GET /api/projects 200 16ms request_id=4f20" },
];

const COMMANDS: Record<string, DemoLine[]> = {
  "bin/rails db:migrate:status": [
    { text: "database: acme_production" },
    { text: "" },
    { text: " Status   Migration ID    Migration Name" },
    { text: "--------------------------------------------------" },
    { text: "   up     20261101093000  Create projects" },
    { text: "   up     20261104120500  Add deployments" },
    { text: "warning: 1 migration is pending", stderr: true, tone: "warn" },
    { text: "  down    20261110081500  Add receipts" },
  ],
  "printenv LOG_LEVEL": [{ text: "debug" }],
};

/** The images the next commit deploys, as a plan lists them under its stack. */
const IMAGE_LINES = [
  "  stack:  app (61f0a2d9c4b7e8135a6c0f2e9d8b4a7c3e1f5b9d2a6c8e0f4b7d1a3c5e9f2b6d)",
  `    image: backend/api -> registry.example.com/acme/api:${NEXT_COMMIT}`,
  `    image: frontend/web -> registry.example.com/acme/web:${NEXT_COMMIT}`,
  `    image: backend/worker -> registry.example.com/acme/api:${NEXT_COMMIT}`,
  "    image: data/postgres -> postgres:17",
  "    image: data/valkey -> valkey/valkey:8",
];

/** What `mm deploy plan` prints for an environment, as the review shows it. */
function planLines(environment: DemoEnvironment): string[] {
  const domains = environment.domains ?? {};
  const routes = [
    `    https://${domains.web} -> app/web port 3000`,
    `    https://${domains.api} -> app/api port 8080`,
  ];
  if (environment.nodes) {
    const names = environment.nodes.map((node) => node.name);
    const node = (name: string, section: string, services: string, networks: string[]) => [
      "",
      `Node ${name}  mmd 0.1.0  section ${section}`,
      "  access:   unchanged (3 user(s))",
      `  services: ${services}`,
      "  networks:",
      ...networks,
      `Deployment plan ${section.slice(8)}${section.slice(0, 8)}`,
      `  ${PROJECT} [${environment.name}] -> local`,
      "  environment delivery: env-file (from config)",
      ...IMAGE_LINES.slice(0, 1),
    ];
    return [
      `Cluster deployment plan ${PLAN_HASH}`,
      `  target:   ${environment.target} (${names.join(", ")})`,
      `  commit:   ${NEXT_COMMIT_FULL}`,
      "  expires:  2026-11-15T14:21:07Z",
      "  networks span the nodes: addresses from 10.64.0.0/12, between nodes through mm0 (MTU 1420)",
      "  service names: each node answers its own containers, on its address on each network (port 53), with a service's healthy containers on the same node, or else on every node",
      `  ingress: ${names[0]} takes the traffic on ports 80 and 443 and sends each request to a healthy container on the same node, else on any node; HTTPS uses a certificate from ACME (Let's Encrypt) for each HTTPS host`,
      ...routes,
      `  rollout: app on ${names.join(", then ")}`,
      "           a stack may take 10m on a node to become healthy (deploy.convergence-timeout); the next node starts once the others resolve its containers",
      "           on failure every node this deployment changed returns to what it ran before",
      "  app: web starts before the old container stops (update.order: start-first)",
      ...node(
        names[0]!,
        "3c9e41b7d20a6f58e1c4b9a7d3f02e6c8b5a1d4f7e9c2b06a3d8f1e5c7b4a92d",
        "app/api, app/web",
        [
          "    backend: keeps 10.64.0.0/24 of 10.64.0.0/20",
          "    frontend: keeps 10.64.16.0/24 of 10.64.16.0/20",
          "    ingress: keeps 10.64.64.0/24 of 10.64.64.0/20",
        ],
      ),
      ...node(
        names[1]!,
        "b17d2e9f04c8a6b3e5d1f7a2c9e4b80d6f3a1c5e7b9d2f4a6c8e0b1d3f5a7c9e",
        "app/api, app/web, app/worker",
        [
          "    backend: keeps 10.64.1.0/24 of 10.64.0.0/20",
          "    frontend: keeps 10.64.17.0/24 of 10.64.16.0/20",
          "    ingress: keeps 10.64.65.0/24 of 10.64.64.0/20",
        ],
      ),
      ...node(
        names[2]!,
        "e40a9c3f7b1d5e8a2c6f0b4d9e3a7c1f5b8d2e6a0c4f9b3d7e1a5c8f2b6d0e4a",
        "app/postgres, app/valkey",
        ["    backend: keeps 10.64.2.0/24 of 10.64.0.0/20"],
      ),
    ];
  }
  if (environment.host) {
    return [
      `Remote deployment plan ${PLAN_HASH}`,
      `  target:   ${environment.target} (d6ba75f6c510f6ff674323f5f1959a23137b8989586e2d6de58259f06d2ca454)`,
      "  daemon:   bd059f889feec28cd94d430c618cefd6cfee1cb3ce6211664d3afc8dc5c6167c protocol=16 version=0.1.0",
      `  commit:   ${NEXT_COMMIT_FULL}`,
      "  expires:  2026-11-15T14:21:07Z",
      "  access:   unchanged (3 user(s))",
      `Deployment plan 52c3efa68dfd16b332bfe8abf9eb3ca92e0f805e15dc0c04187f3b9c830a5a46`,
      `  ${PROJECT} [${environment.name}] -> local`,
      "  environment delivery: env-file (from config)",
      "  health: a stack may take 10m to become healthy (deploy.convergence-timeout); on failure the deployment is rolled back",
      ...IMAGE_LINES,
      "  routes: unchanged",
      "  ingress:",
      `    app/api: https://${domains.api} -> port 8080 (explicit config)`,
      `    app/web: https://${domains.web} -> port 3000 (explicit config)`,
      "  ingress: managed",
    ];
  }
  return [
    `Deployment plan ${LOCAL_PLAN_HASH}`,
    `  ${PROJECT} [${environment.name}] -> local`,
    "  environment delivery: env-file (from development-default)",
    ...IMAGE_LINES,
    `  + https://api.${PROJECT}.mm.test/ -> app/api:8080`,
    `  + https://web.${PROJECT}.mm.test/ -> app/web:3000`,
    "  ingress:",
    `    app/api: https://api.${PROJECT}.mm.test -> port 8080 (explicit config)`,
    "    app/postgres: not routed - service role 'database' is not http-routable",
    "    app/valkey: not routed - service role 'database' is not http-routable",
    `    app/web: https://web.${PROJECT}.mm.test -> port 3000 (automatic convention)`,
    "    app/worker: not routed - service role 'worker' is not http-routable",
    "  ingress: managed",
  ];
}

/** `desired` is the count on one host; `placement` is where the service runs on the cluster. */
const SERVICE_SEED: {
  name: string;
  module: string;
  stack: string;
  desired: number;
  checked: boolean;
  placement: string[];
}[] = [
  { name: "web", module: "frontend", stack: "app", desired: 2, checked: true, placement: ["app-1", "app-2"] },
  { name: "api", module: "backend", stack: "app", desired: 3, checked: true, placement: ["app-1", "app-2"] },
  { name: "worker", module: "backend", stack: "app", desired: 2, checked: false, placement: ["app-2"] },
  { name: "postgres", module: "data", stack: "app", desired: 1, checked: true, placement: ["db-1"] },
  { name: "valkey", module: "data", stack: "app", desired: 1, checked: true, placement: ["db-1"] },
];

/** The services of an environment as a tour finds them: deployed on a host, stopped on this machine. */
function seed(environment: DemoEnvironment): DemoService[] {
  return SERVICE_SEED.map(({ placement, ...service }) => {
    if (environment.nodes) {
      const count = placement.length;
      return { ...service, desired: count, running: count, present: true, restarts: 0, nodes: placement };
    }
    const up = Boolean(environment.host);
    return { ...service, running: up ? service.desired : 0, present: up, restarts: 0 };
  });
}

const ROUTES: DemoRoute[] = [
  {
    service: "web",
    stack: "app",
    url: `https://web.${PROJECT}.mm.test`,
    port: 3000,
    source: "automatic convention",
  },
  {
    service: "api",
    stack: "app",
    url: `https://api.${PROJECT}.mm.test`,
    port: 8080,
    source: "explicit config",
  },
  { service: "worker", stack: "app", reason: "service role 'worker' is not http-routable" },
  { service: "postgres", stack: "app", reason: "service role 'database' is not http-routable" },
  { service: "valkey", stack: "app", reason: "service role 'database' is not http-routable" },
];

const VARIABLES: DemoVariable[] = [
  {
    service: "api",
    stack: "app",
    first: true,
    name: "DATABASE_URL",
    status: "resolved",
    source: "config",
    value: "postgres://postgres/acme",
    key: "database.url",
    line: 3,
  },
  {
    service: "api",
    stack: "app",
    first: false,
    name: "CACHE_URL",
    status: "resolved",
    source: "config",
    value: "redis://valkey:6379/0",
    key: "cache.url",
    line: 4,
  },
  {
    service: "api",
    stack: "app",
    first: false,
    name: "SESSION_SECRET",
    status: "resolved",
    source: "secret",
    sensitive: true,
    key: "session.secret",
    line: 5,
  },
  {
    service: "api",
    stack: "app",
    first: false,
    name: "PAYMENTS_API_KEY",
    status: "resolved",
    source: "secret",
    sensitive: true,
    key: "payments.api_key",
    line: 6,
  },
  {
    service: "api",
    stack: "app",
    first: false,
    name: "LOG_LEVEL",
    status: "resolved",
    source: "local override",
    value: "debug",
    key: "log.level",
    line: 7,
  },
  {
    service: "web",
    stack: "app",
    first: true,
    name: "API_URL",
    status: "resolved",
    source: "config",
    value: `https://api.${PROJECT}.mm.test`,
    key: "api.url",
    line: 2,
  },
  {
    service: "web",
    stack: "app",
    first: false,
    name: "SITE_NAME",
    status: "defaulted",
    source: "template default",
    value: "Acme",
    line: 3,
  },
  {
    service: "worker",
    stack: "app",
    first: true,
    name: "DATABASE_URL",
    status: "resolved",
    source: "config",
    value: "postgres://postgres/acme",
    key: "database.url",
    line: 2,
  },
  {
    service: "worker",
    stack: "app",
    first: false,
    name: "QUEUE_CONCURRENCY",
    status: "defaulted",
    source: "template default",
    value: "4",
    line: 3,
  },
  {
    service: "worker",
    stack: "app",
    first: false,
    name: "MAIL_URL",
    status: "resolved",
    source: "config",
    value: "smtp://mailpit:1025",
    key: "mail.url",
    line: 4,
  },
];

export interface DemoConfirm {
  title: string;
  lines: string[];
  accept: string;
  action: () => void;
}

interface Step {
  /** Milliseconds to wait before this step runs. */
  wait: number;
  run: () => void;
}

export function useTuiDemo() {
  const tab = ref<TabId>("overview");
  const scroll = ref(0);
  const busy = ref<string>();
  const confirm = ref<DemoConfirm>();
  const review = ref<DemoReview>();
  const help = ref(false);
  const notice = ref<{ tone: Tone; text: string }>();
  const lastKey = ref<string>();

  // Which environment every view shows, and the one a switch is loading.
  const environment = ref(HOME);
  const switching = ref<string>();
  const current = computed(() => ENVIRONMENTS.find((entry) => entry.name === environment.value)!);
  /** The remote target's name, or undefined when the environment runs on this machine. */
  const remote = computed(() => (isRemote(current.value) ? current.value.target : undefined));
  /** The nodes of the target on screen, when it is a cluster. */
  const nodes = computed(() => current.value.nodes ?? []);

  // The stack on this machine keeps its state across switches; a remote environment is read anew.
  const local = ref(seed(ENVIRONMENTS[0]!));
  const hosted = ref(seed(current.value));
  const services = computed(() => (remote.value ? hosted.value : local.value));
  /** The commit each remote environment runs. */
  const deployed = ref({ ...DEPLOYED });

  // Service actions: the menu, the open log or command view, and the command prompt.
  const collapsed = ref<string[]>([]);
  const menu = ref<{ label: string; service: string; selected: number }>();
  const panel = ref<DemoPanel>();
  const prompt = ref<{ label: string; input: string }>();
  const restarting = ref<string>();
  const filter = ref("");

  // The team in the users file, and the form that adds to it.
  const team = ref<DemoMember[]>(TEAM_SEED.map((member) => ({ ...member })));
  const editor = ref<DemoEditor>();

  const present = computed(() => services.value.filter((service) => service.present));
  /** Services whose every container runs and passes its health check. */
  const runningCount = computed(
    () => present.value.filter((service) => ready(service) === service.desired).length,
  );
  const allRunning = computed(() => present.value.length > 0 && runningCount.value === present.value.length);
  const stopped = computed(() => present.value.every((service) => service.running === 0));
  /** 0..1: how much of the stack is up. Drives the glow behind the terminal. */
  const vitality = computed(() => {
    const desired = services.value.reduce((sum, service) => sum + service.desired, 0);
    const running = services.value.reduce((sum, service) => sum + service.running, 0);
    return running / desired;
  });

  /**
   * Services and the rows beneath each one that runs several containers: on this machine
   * its containers, which can be selected; on a cluster one muted row per node.
   */
  const serviceRows = computed<ServiceRow[]>(() =>
    present.value.flatMap((service) => {
      const expanded = foldable(service) && !collapsed.value.includes(service.name);
      const rows: ServiceRow[] = [{ kind: "service", service, expanded }];
      if (!expanded) return rows;
      if (service.nodes) {
        service.nodes.forEach((node, index) => {
          const starting = service.replacing === node;
          const fresh = Boolean(service.fresh?.includes(node));
          rows.push({
            kind: "node",
            service,
            node,
            container: `${service.name}-1`,
            health: starting ? "starting" : service.checked ? "healthy" : "running",
            status: starting ? "Up 2 seconds" : fresh ? "Up 9 seconds" : "Up 2 hours",
            fresh,
            last: index + 1 === service.nodes!.length,
          });
        });
        return rows;
      }
      for (let index = 0; index < service.desired; index += 1) {
        rows.push({
          kind: "container",
          service,
          name: `${PROJECT}-${service.name}-${index + 1}`,
          state: index < service.running ? "running" : "exited",
          last: index + 1 === service.desired,
        });
      }
      return rows;
    }),
  );
  const selectedRow = computed(() => serviceRows.value[scroll.value]);

  /** Why the host refuses a command in this service, in the TUI's words; nothing on this machine. */
  function execRefusal(service: string): string | undefined {
    if (!remote.value) return undefined;
    const grant = team.value.find((member) => member.you)?.access[environment.value] ?? "";
    const named = /\+exec\[([^\]]*)\]/.exec(grant);
    if (named) {
      const allowed = named[1]!.split(",").map((name) => name.trim());
      return allowed.includes(service)
        ? undefined
        : `The host grants you exec only on ${allowed.join(", ")}.`;
    }
    if (grant.includes("+exec")) return undefined;
    return `The host does not grant you exec on '${environment.value}'. Ask for +exec in the users file.`;
  }

  const menuItems = computed<MenuItem[]>(() => {
    const row = selectedRow.value;
    // A node's row names its container, like a container's row on one host.
    const replica = row?.kind === "container" || row?.kind === "node";
    return [
      { key: "l", label: "Logs", detail: replica ? "follow this container" : "follow every replica" },
      {
        key: "x",
        label: "Run a command",
        detail: "no TTY; output is captured",
        disabled: row ? execRefusal(row.service.name) : undefined,
      },
      {
        key: "r",
        label: "Restart",
        detail: replica ? "this container only" : "every container of the service",
      },
      { key: "v", label: "Variables", detail: "what the service reads, and from where" },
    ];
  });

  const routes = computed<DemoRoute[]>(() =>
    ROUTES.filter((route) => present.value.some((service) => service.name === route.service)).map((route) => {
      const service = services.value.find((candidate) => candidate.name === route.service);
      const domains = current.value.domains;
      if (domains) {
        // A remote environment serves the routes its `ingress:` names, and makes none itself.
        const host = domains[route.service];
        return host
          ? {
              service: route.service,
              stack: route.stack,
              url: `https://${host}`,
              port: route.port,
              source: "explicit config",
            }
          : {
              service: route.service,
              stack: route.stack,
              reason: `no route of stacks.app.ingress names it in '${environment.value}'; mm adds routes by itself only in the development environment (development)`,
            };
      }
      return route.url && service && service.running === 0
        ? { service: route.service, stack: route.stack, reason: "service is not running" }
        : route;
    }),
  );
  const routed = computed(() => routes.value.filter((route) => route.url).length);

  const variables = computed(() => {
    const shown = VARIABLES.filter((variable) => !filter.value || variable.service === filter.value);
    return shown.map((variable, index) => ({
      ...variable,
      first: index === 0 || shown[index - 1]!.service !== variable.service,
    }));
  });

  const diagnostics = computed(() => {
    const found: { tone: Tone; message: string; code: string; help?: string }[] = [];
    const worker = services.value.find((service) => service.name === "worker");
    if (worker && worker.present && worker.restarts > 0 && worker.running < worker.desired) {
      found.push({
        tone: "warn",
        message: "worker is restarting repeatedly",
        code: "runtime.service.restarting",
        help: "Run mm logs worker to see why it exits.",
      });
    }
    found.push({
      tone: "info",
      message: "Environment delivery uses the development default",
      code: "config.defaulted",
    });
    return found;
  });
  const issues = computed(() => diagnostics.value.filter((item) => item.tone !== "info"));

  /** What a member may do in the environment on screen. */
  function accessHere(member: DemoMember): string | undefined {
    return member.access[environment.value] ?? (remote.value ? member.access["*"] : undefined);
  }

  const rowCount = computed(() => {
    switch (tab.value) {
      case "environments":
        return ENVIRONMENTS.length;
      case "services":
        return serviceRows.value.length;
      case "ingress":
        return routes.value.length;
      case "variables":
        return variables.value.length;
      case "diagnostics":
        return diagnostics.value.length;
      case "access":
        return team.value.length;
      case "mounts":
        return 3;
      case "gates":
        return 2;
      case "providers":
        return 2;
      case "jobs":
        return 3;
      default:
        return 0;
    }
  });

  /** Whether keys go to a text field, which hides the global hints. */
  const typing = computed(() => Boolean(prompt.value || editor.value || switching.value));

  const hints = computed<Hint[]>(() => {
    const select: Hint = ["j/k", "select"];
    const scrolling: Hint = ["j/k", "scroll"];
    const edit: Hint[] = [
      ["a", "add"],
      ["e", "edit"],
      ["D", "remove"],
    ];
    // On a remote environment `u` deploys this commit, and nothing stops the stack.
    const stack: Hint[] = remote.value
      ? [["u", "deploy"]]
      : [
          ["u", "start"],
          ["d", "stop"],
        ];
    if (switching.value) return [["esc", "cancel"]];
    if (review.value?.shown) {
      const scrollPlan: Hint = ["j/k", "scroll"];
      if (review.value.stage === "reviewing") {
        return [scrollPlan, ["enter", review.value.remote ? "deploy" : "start"], ["esc", "cancel"]];
      }
      return review.value.stage === "applying"
        ? [scrollPlan, ["esc", "hide; u shows it again"]]
        : [scrollPlan, ["enter", "close"]];
    }
    if (editor.value) {
      if (editor.value.stage === "form") {
        return [
          ["↑↓", "field"],
          ["enter", "review"],
          ["esc", "cancel"],
        ];
      }
      return editor.value.stage === "review"
        ? [
            ["enter", "apply"],
            ["esc", "cancel"],
          ]
        : [
            ["enter", "done"],
            ["esc", "close"],
          ];
    }
    if (prompt.value) {
      return [
        ["enter", "run"],
        ["↑↓", "history"],
        ["esc", "cancel"],
      ];
    }
    if (menu.value) return [select, ["enter", "choose"], ["esc", "close"]];
    if (panel.value?.kind === "logs") {
      return [
        scrolling,
        ["space", panel.value.following ? "pause" : "follow"],
        ["/", "filter"],
        ["esc", "close"],
        ["w", "wrap"],
        ["t", "timestamps"],
        ["c", "clear"],
      ];
    }
    if (panel.value) {
      return [["enter", "run another"], ["r", "run again"], scrolling, ["esc", "close"]];
    }
    switch (tab.value) {
      case "overview":
        return [...stack, ["w", "guided setup"]];
      case "tls":
        return [["w", "guided setup"]];
      case "environments":
        return [select, ["enter", "switch"], ["r", "check again"], ...edit];
      case "services":
        return [
          select,
          ["enter", "actions"],
          ["l", "logs"],
          ["x", "run"],
          ["r", "restart"],
          ["v", "variables"],
          ["space", "fold"],
          ...stack,
        ];
      case "variables":
        return remote.value
          ? [
              select,
              ["/", "search"],
              ["enter", "set in config"],
              ["D", "remove from config"],
              ["p", "problems"],
              ...stack,
            ]
          : [
              select,
              ["/", "search"],
              ["enter", "override"],
              ["x", "remove override"],
              ["c", "set in config"],
              ["D", "remove from config"],
              ["p", "problems"],
              ...stack,
            ];
      case "ingress":
      case "diagnostics":
        return [select, ["pgup/pgdn", "page"]];
      case "mounts":
        return [select, ["c", "configure"], ["d", "disable"]];
      case "access":
        return [select, ["a", "add teammate"], ["e", "edit"], ["D", "remove"]];
      case "gates":
        return [select, ["a", "add user"]];
      case "providers":
        return [select];
      case "jobs":
        return [["h/l", "pane"], select, ["r", "run"], ["x", "reconcile"], ["t", "tick"], ...edit];
      case "guided":
        return [
          ["g/i/p/n/e", "start"],
          ["enter", "continue"],
          ["b", "back"],
          ["esc", "cancel"],
        ];
    }
  });

  // --- Timeline ----------------------------------------------------------------------

  let timers: ReturnType<typeof setTimeout>[] = [];
  let resume: ReturnType<typeof setTimeout> | undefined;
  let stream: ReturnType<typeof setInterval> | undefined;
  let sequence = 0;
  let logCursor = 0;
  const autoplay = ref(true);

  function clearTimers() {
    for (const timer of timers) clearTimeout(timer);
    timers = [];
  }

  /** Runs steps in order; a newer call or `clearTimers` abandons the rest. */
  function play(steps: Step[], done?: () => void) {
    const mine = ++sequence;
    let at = 0;
    for (const step of steps) {
      at += step.wait;
      timers.push(
        setTimeout(() => {
          if (mine === sequence) step.run();
        }, at),
      );
    }
    if (done) {
      timers.push(
        setTimeout(() => {
          if (mine === sequence) done();
        }, at + 1),
      );
    }
  }

  /**
   * Runs steps inside whatever is playing, without abandoning it: an action the tour
   * triggers (a switch, a command, a restart) must not cancel the rest of the tour.
   */
  function animate(steps: Step[], done?: () => void) {
    const mine = sequence;
    let at = 0;
    for (const step of steps) {
      at += step.wait;
      timers.push(
        setTimeout(() => {
          if (mine === sequence) step.run();
        }, at),
      );
    }
    if (done) {
      timers.push(
        setTimeout(() => {
          if (mine === sequence) done();
        }, at + 1),
      );
    }
  }

  function flash(key: string) {
    lastKey.value = key;
    timers.push(
      setTimeout(() => {
        if (lastKey.value === key) lastKey.value = undefined;
      }, 700),
    );
  }

  function open(next: TabId) {
    tab.value = next;
    scroll.value = next === "environments" ? ENVIRONMENTS.indexOf(current.value) : 0;
    help.value = false;
    if (next !== "variables") filter.value = "";
  }

  function allUp() {
    for (const service of services.value) {
      service.present = true;
      service.running = service.desired;
      service.replacing = undefined;
    }
  }

  function startSteps(): Step[] {
    const steps: Step[] = [
      {
        wait: 0,
        run: () => {
          busy.value = "Starting stack";
          notice.value = undefined;
        },
      },
    ];
    // Data services come up first, then the app replicas one at a time.
    for (const name of ["postgres", "valkey", "api", "web", "worker"]) {
      steps.push({
        wait: 260,
        run: () => {
          const service = services.value.find((candidate) => candidate.name === name);
          if (service) service.present = true;
        },
      });
    }
    for (const name of ["postgres", "valkey", "api", "api", "web", "api", "web", "worker", "worker"]) {
      steps.push({
        wait: 300,
        run: () => {
          const service = services.value.find((candidate) => candidate.name === name);
          if (service && service.running < service.desired) service.running += 1;
        },
      });
    }
    steps.push({
      wait: 420,
      run: () => finishReview(`Started app on local (${plural(routeCount(current.value), "route")}).`),
    });
    return steps;
  }

  /** The apply ended: the review comes back with what it said. */
  function finishReview(summary: string) {
    busy.value = undefined;
    const open = review.value;
    if (!open) return;
    for (const row of open.nodes) {
      row.stage = "done";
      row.step = undefined;
    }
    open.stage = "applied";
    open.summary = summary;
    open.shown = true;
    open.scroll = 0;
  }

  /** What one node of the apply is doing, as its row in the review says. */
  function say(node: string, stage: DemoReviewNode["stage"], step?: string) {
    const row = review.value?.nodes.find((candidate) => candidate.node === node);
    if (!row) return;
    row.stage = stage;
    row.step = step;
    if (stage === "waiting" && !row.stacks.includes("app")) row.stacks.push("app");
  }

  /**
   * Replaces the changed services one node at a time, in the order the target lists the
   * nodes. A new container counts once it is healthy; only then does the next node start.
   * Then every node finishes, in the same order.
   */
  function rolloutSteps(target: string): Step[] {
    const steps: Step[] = [{ wait: 0, run: () => (busy.value = "Deploying") }];
    for (const node of nodes.value) {
      const here = () =>
        services.value.filter(
          (service) => service.nodes?.includes(node.name) && CHANGED.includes(service.name),
        );
      // In the order `mm deploy apply` prints them: preparing, the start-first note, the stack.
      steps.push({ wait: 500, run: () => say(node.name, "working", "preparing") });
      if (here().length) {
        steps.push({
          wait: 600,
          run: () => {
            for (const service of here()) service.replacing = node.name;
            say(node.name, "working", "app: starting the new web next to the old");
          },
        });
        steps.push({
          wait: 1500,
          run: () => {
            for (const service of here()) {
              if (service.replacing !== node.name) continue;
              service.replacing = undefined;
              service.fresh = [...(service.fresh ?? []), node.name];
            }
            say(node.name, "working", "applies app");
          },
        });
      } else steps.push({ wait: 600, run: () => say(node.name, "working", "applies app") });
      steps.push({ wait: 700, run: () => say(node.name, "waiting") });
    }
    nodes.value.forEach((node, index) => {
      steps.push({
        wait: 450,
        run: () => {
          const before = nodes.value[index - 1];
          if (before) say(before.name, "done");
          say(node.name, "working", "finishing");
        },
      });
    });
    steps.push({
      wait: 600,
      run: () => {
        deployed.value[environment.value] = NEXT_COMMIT;
        finishReview(`Applied ${NEXT_PLAN} on ${nodes.value.length} node(s) of ${target}.`);
      },
    });
    return steps;
  }

  /** One remote host: the services it changes are replaced, then the review says so. */
  function hostSteps(target: string): Step[] {
    return [
      { wait: 0, run: () => (busy.value = "Deploying") },
      {
        wait: 900,
        run: () => {
          for (const service of services.value) if (CHANGED.includes(service.name)) service.running -= 1;
        },
      },
      {
        wait: 1400,
        run: () => {
          for (const service of services.value) service.running = service.desired;
        },
      },
      {
        wait: 600,
        run: () => {
          deployed.value[environment.value] = NEXT_COMMIT;
          finishReview(`Started app on ${target} (${plural(routeCount(current.value), "route")}).`);
        },
      },
    ];
  }

  /** The review `u` opens: the plan as `mm deploy plan` prints it, before anything changes. */
  function openReview() {
    const environment = current.value;
    const names = nodes.value.map((node) => node.name);
    review.value = {
      remote: Boolean(remote.value),
      stage: "reviewing",
      head: `app on target '${environment.target}'${names.length ? ` (${names.join(", ")})` : ""}`,
      routes: routeCount(environment),
      plan: remote.value ? NEXT_PLAN : LOCAL_PLAN_HASH.slice(0, 12),
      lines: planLines(environment),
      nodes: names.map((node) => ({ node, stage: "waiting", stacks: [] })),
      scroll: 0,
      shown: true,
    };
  }

  /** `enter` on the review: apply exactly the plan it shows. */
  function applyReview() {
    const open = review.value;
    if (!open || open.stage !== "reviewing" || busy.value) return;
    open.stage = "applying";
    open.scroll = 0;
    const target = current.value.target;
    if (nodes.value.length) animate(rolloutSteps(target));
    else if (remote.value) animate(hostSteps(target));
    else animate(startSteps());
  }

  function start() {
    if (busy.value) return;
    if (remote.value && deployed.value[environment.value] === NEXT_COMMIT) {
      notice.value = { tone: "info", text: `${remote.value} already runs commit ${NEXT_COMMIT}.` };
      return;
    }
    if (!remote.value && allRunning.value) {
      notice.value = { tone: "info", text: "The stack is already running" };
      return;
    }
    animate([
      { wait: 0, run: () => (busy.value = "Reviewing deployment plan") },
      {
        wait: 900,
        run: () => {
          busy.value = undefined;
          openReview();
        },
      },
    ]);
  }

  function stop() {
    play([
      { wait: 0, run: () => (busy.value = "Stopping stack") },
      {
        wait: 900,
        run: () => {
          for (const service of services.value) {
            service.running = 0;
            service.present = false;
            service.restarts = 0;
          }
          busy.value = undefined;
          notice.value = { tone: "ok", text: "Stack stopped" };
          scroll.value = 0;
        },
      },
    ]);
  }

  // --- Service actions -----------------------------------------------------------------

  function stopStream() {
    clearInterval(stream);
    stream = undefined;
  }

  /** The next line of the stream; with several nodes, lines arrive from each in turn. */
  function nextLogLine(sources: { node: string; container: string }[]): DemoLine {
    const line = LOG_POOL[logCursor % LOG_POOL.length]!;
    const seconds = (logCursor * 3) % 60;
    // Not a plain round robin: two lines in a row from one node look like a real stream.
    const source = sources.length
      ? sources[(logCursor * 2 + (logCursor % 3 ? 0 : 1)) % sources.length]
      : undefined;
    logCursor += 1;
    return { ...line, ...source, time: `14:03:${String(seconds).padStart(2, "0")}` };
  }

  function openLogs(row: ServiceRow) {
    stopStream();
    menu.value = undefined;
    logCursor = 0;
    // A service on a cluster is all of its containers on all of its nodes, merged in time order;
    // a node's row follows the container on that node.
    const on = row.kind === "container" ? [] : row.kind === "node" ? [row.node] : (row.service.nodes ?? []);
    const sources = on.map((node) => ({ node, container: `${row.service.name}-1` }));
    const logs: DemoPanel = {
      kind: "logs",
      label: rowLabel(row),
      where: row.kind === "node" ? `on ${row.node}` : undefined,
      nodes: on,
      lines: Array.from({ length: 6 }, () => nextLogLine(sources)),
      following: true,
      unseen: 0,
      times: false,
    };
    panel.value = logs;
    stream = setInterval(() => {
      const view = panel.value;
      if (view?.kind !== "logs") return stopStream();
      if (!view.following) {
        view.unseen += 1;
        return;
      }
      view.lines.push(nextLogLine(sources));
      // Keep what fits; the real view keeps the newest 5,000 lines.
      if (view.lines.length > 60) view.lines.shift();
    }, 850);
  }

  function runCommand(label: string, command: string) {
    stopStream();
    prompt.value = undefined;
    const lines = COMMANDS[command] ?? [
      {
        text: `This preview has no containers. In mm, the output of that command appears here.`,
        tone: "info",
      },
    ];
    panel.value = { kind: "run", label, command, lines: [] };
    animate([
      ...lines.map((line, index) => ({
        wait: index === 0 ? 350 : 90,
        run: () => {
          if (panel.value?.kind === "run") panel.value.lines.push(line);
        },
      })),
      {
        wait: 250,
        run: () => {
          if (panel.value?.kind === "run") panel.value.exit = 0;
        },
      },
    ]);
  }

  function closePanel() {
    stopStream();
    panel.value = undefined;
  }

  /** The container a command runs in: the selected one, or a service's first. */
  function execLabel(row: ServiceRow): string {
    if (row.kind === "node") return `${row.container} on ${row.node}`;
    const node = row.service.nodes?.[0];
    if (node) return `${row.service.name} on ${node}`;
    if (row.kind === "container") return row.name;
    return row.service.desired > 1 ? `${PROJECT}-${row.service.name}-1` : row.service.name;
  }

  /**
   * Restarts a container, or every container of a service. On a cluster the service restarts
   * one node at a time, each healthy again before the next, and says so in the TUI's words.
   */
  function restart(row: ServiceRow) {
    const service = row.service;
    const label = rowLabel(row);
    const single = row.kind !== "service";
    const count = single ? 1 : service.nodes ? service.nodes.length : service.desired;
    confirm.value = {
      title: `Restart ${label}?`,
      lines: [
        single
          ? `Restarts the container ${label}.`
          : count === 1
            ? `Restarts the container of ${service.name}.`
            : `Restarts all ${count} containers of ${service.name}.`,
        "Requests in flight are dropped.",
        "The image, configuration, and volumes stay as deployed.",
      ],
      accept: "restart",
      action: () => {
        const on = row.kind === "node" ? [row.node] : row.kind === "service" ? (service.nodes ?? []) : [];
        const containers = plural(count, "container");
        const summary =
          on.length > 1
            ? `Restarted ${service.name} on ${on.join(", then ")}, one node at a time, each healthy before the next (${containers}).`
            : on.length === 1
              ? `Restarted ${service.name} on ${on[0]} (${containers}); it is healthy.`
              : `Restarted ${service.name} (${containers}); it is healthy.`;
        const steps: Step[] = [{ wait: 0, run: () => (restarting.value = service.name) }];
        if (on.length) {
          // Each node's container leaves, restarts, and is healthy again before the next.
          for (const node of on) {
            steps.push({ wait: 500, run: () => (service.replacing = node) });
            steps.push({
              wait: 1300,
              run: () => {
                service.replacing = undefined;
                service.fresh = [...(service.fresh ?? []).filter((name) => name !== node), node];
              },
            });
          }
        } else {
          const before = service.running;
          steps.push({ wait: 0, run: () => (service.running = Math.max(before - count, 0)) });
          for (let index = 0; index < count; index += 1) {
            steps.push({ wait: 450, run: () => (service.running = Math.min(service.running + 1, before)) });
          }
        }
        steps.push({
          wait: 300,
          run: () => {
            restarting.value = undefined;
            notice.value = { tone: "ok", text: summary };
          },
        });
        animate(steps);
      },
    };
  }

  /** What the TUI calls a row in a menu, a dialog, or a view's title. */
  function rowLabel(row: ServiceRow): string {
    if (row.kind === "container") return row.name;
    if (row.kind === "node") return row.container;
    return row.service.name;
  }

  /** Runs one of the menu's actions on the selected row; `false` when nothing is selected. */
  function act(key: string): boolean {
    const row = selectedRow.value;
    if (!row) return false;
    const item = menuItems.value.find((candidate) => candidate.key === key);
    if (!item) return false;
    menu.value = undefined;
    if (item.disabled) {
      notice.value = { tone: "info", text: item.disabled };
      return true;
    }
    if (key === "l") openLogs(row);
    else if (key === "x") prompt.value = { label: execLabel(row), input: "" };
    else if (key === "r") restart(row);
    else {
      open("variables");
      filter.value = row.service.name;
    }
    return true;
  }

  // --- Environments ----------------------------------------------------------------------

  function switchTo(name: string, then?: () => void) {
    const next = ENVIRONMENTS.find((entry) => entry.name === name);
    if (!next) return;
    if (name === environment.value) {
      notice.value = { tone: "info", text: `You are already on ${name}.` };
      return;
    }
    switching.value = name;
    animate(
      [
        {
          wait: isRemote(next) ? 1300 : 600,
          run: () => {
            switching.value = undefined;
            environment.value = name;
            // An environment someone deployed is up; the local one keeps its state.
            if (isRemote(next)) hosted.value = seed(next);
            notice.value = {
              tone: "ok",
              text: `Switched to ${name} (${isRemote(next) ? `remote · ${next.target}` : "local"}).`,
            };
          },
        },
      ],
      then,
    );
  }

  // --- Team ----------------------------------------------------------------------------

  const remoteEnvironments = ENVIRONMENTS.filter(isRemote).map((entry) => entry.name);
  /** Form fields in order: name, public key, then one level per remote environment. */
  const editorFields = ["name", "key", ...remoteEnvironments];

  function openEditor() {
    editor.value = {
      stage: "form",
      focus: 0,
      name: "",
      key: DEMO_KEY,
      levels: Object.fromEntries(remoteEnvironments.map((name) => [name, "none"])),
    };
  }

  /** The lines the change adds to the users file. */
  const editorDiff = computed(() => {
    const form = editor.value;
    if (!form) return [];
    const granted = remoteEnvironments.filter((name) => form.levels[name] !== "none");
    return [
      `${form.name}:`,
      "  keys:",
      `    - ${form.key}`,
      "  access:",
      ...granted.map((name) => `    ${name}: ${form.levels[name]}`),
    ];
  });

  function editorKey(name: string): boolean {
    const form = editor.value;
    if (!form) return false;
    if (name === "Escape") {
      editor.value = undefined;
      return true;
    }
    if (form.stage === "done") {
      if (name === "Enter") editor.value = undefined;
      return true;
    }
    if (form.stage === "review") {
      if (name === "b") form.stage = "form";
      else if (name === "Enter" || name === "y") {
        team.value.push({
          name: form.name,
          access: Object.fromEntries(Object.entries(form.levels).filter(([, level]) => level !== "none")),
          keys: 1,
        });
        form.stage = "done";
      }
      return true;
    }
    form.error = undefined;
    const field = editorFields[form.focus]!;
    const text = field === "name" || field === "key" ? field : undefined;
    if (name === "ArrowDown") form.focus = (form.focus + 1) % editorFields.length;
    else if (name === "ArrowUp") form.focus = (form.focus + editorFields.length - 1) % editorFields.length;
    else if (name === "Enter") {
      if (!/^[a-z][a-z0-9-]*$/.test(form.name)) {
        form.error = form.name
          ? `user name '${form.name}' must match [a-z][a-z0-9-]{0,62}`
          : "Name is required.";
      } else if (Object.values(form.levels).every((level) => level === "none")) {
        form.error = "Choose a level for at least one environment (← → changes it).";
      } else form.stage = "review";
    } else if (!text && (name === "ArrowRight" || name === "ArrowLeft" || name === " ")) {
      const at = LEVELS.indexOf(form.levels[field] as (typeof LEVELS)[number]);
      const step = name === "ArrowLeft" ? LEVELS.length - 1 : 1;
      form.levels[field] = LEVELS[(at + step) % LEVELS.length]!;
    } else if (text && name === "Backspace") form[text] = form[text].slice(0, -1);
    else if (text && name.length === 1 && form[text].length < 64) form[text] += name;
    return true;
  }

  // --- Tour ------------------------------------------------------------------------------

  /** Puts the demo back where a tour begins, without animating. */
  function reset() {
    stopStream();
    confirm.value = undefined;
    review.value = undefined;
    help.value = false;
    menu.value = undefined;
    panel.value = undefined;
    prompt.value = undefined;
    editor.value = undefined;
    switching.value = undefined;
    restarting.value = undefined;
    filter.value = "";
    busy.value = undefined;
    environment.value = HOME;
    hosted.value = seed(current.value);
    deployed.value = { ...DEPLOYED };
    team.value = TEAM_SEED.map((member) => ({ ...member }));
    open("services");
  }

  /**
   * The unattended tour: roll a deploy through the cluster, read one log merged from its
   * nodes, look at the nodes, then run the same project on this machine.
   */
  function tour() {
    const press = (key: string, run: () => void, wait: number): Step => ({
      wait,
      run: () => {
        flash(key);
        notice.value = undefined;
        run();
      },
    });
    const select = (wait: number): Step => press("j", () => move(1), wait);
    const walk: Step[] = [
      // Review the deploy: the plan as `mm deploy plan` prints it.
      press("u", () => start(), 2600),
      press("j", () => key("j"), 2600),
      press("j", () => key("j"), 300),
      press("j", () => key("j"), 300),
      // Accept it and watch the first node; then hide the review and watch the services.
      press("Enter", () => key("Enter"), 1600),
      press("Escape", () => key("Escape"), 2900),
      // The review comes back when the apply ends; close it.
      press("Enter", () => key("Enter"), 9800),
      // The log of web: both nodes in one stream.
      press("l", () => key("l"), 1400),
      press("Escape", () => key("Escape"), 5600),
      // Restart web on the cluster: one node at a time.
      press("r", () => key("r"), 900),
      press("y", () => key("y"), 1600),
      // Environments: the nodes of production, then over to this machine.
      press("2", () => open("environments"), 5600),
      press("k", () => move(-1), 3000),
      press("k", () => move(-1), 350),
      press("Enter", () => key("Enter"), 800),
      // Start the stack here: the same review, then the services come up.
      press("3", () => open("services"), 1500),
      press("u", () => start(), 1200),
      press("Enter", () => key("Enter"), 2400),
      press("Enter", () => key("Enter"), 5600),
      press("4", () => open("ingress"), 1000),
      // Back to the cluster.
      press("2", () => open("environments"), 4200),
      select(700),
      select(350),
      press("Enter", () => key("Enter"), 700),
      press("3", () => open("services"), 2600),
      { wait: 2400, run: () => undefined },
    ];
    const loop = () => {
      reset();
      play(walk, loop);
    };
    loop();
  }

  /** A person took over: stop the tour and pick it up again after a quiet spell. */
  function takeOver() {
    autoplay.value = false;
    clearTimeout(resume);
    resume = setTimeout(() => {
      // Never restart under someone reading logs or filling in a form.
      if (busy.value || panel.value || prompt.value || editor.value || review.value) return takeOver();
      autoplay.value = true;
      tour();
    }, 14_000);
  }

  /** Moves the selection; a cluster's node rows are rows to act on, like containers on one host. */
  function move(delta: number) {
    if (rowCount.value === 0) return;
    scroll.value = Math.min(Math.max(scroll.value + delta, 0), rowCount.value - 1);
  }

  /** Keys while the review covers the screen. */
  function reviewKey(name: string): boolean {
    const open = review.value!;
    if (help.value) {
      help.value = false;
      return true;
    }
    const last = Math.max(open.lines.length - 1, 0);
    if (name === "j" || name === "ArrowDown") open.scroll = Math.min(open.scroll + 1, last);
    else if (name === "k" || name === "ArrowUp") open.scroll = Math.max(open.scroll - 1, 0);
    else if (name === " " || name === "PageDown") open.scroll = Math.min(open.scroll + 15, last);
    else if (name === "PageUp") open.scroll = Math.max(open.scroll - 15, 0);
    else if (name === "g" || name === "Home") open.scroll = 0;
    else if (name === "?") help.value = true;
    else if ((name === "Enter" || name === "y") && open.stage === "reviewing") applyReview();
    else if (["Escape", "q", "n"].includes(name) && open.stage === "applying") {
      // The apply goes on; the header says so and `u` shows it again.
      open.shown = false;
    } else if (["Escape", "q", "n", "Enter"].includes(name)) review.value = undefined;
    return true;
  }

  /** Keys while a log or command view covers the screen. */
  function panelKey(name: string): boolean {
    const view = panel.value;
    if (!view) return false;
    if (name === "Escape" || name === "q") closePanel();
    else if (view.kind === "logs") {
      if (name === " ") {
        view.following = !view.following;
        view.unseen = 0;
      } else if (name === "t") view.times = !view.times;
      else if (name === "c") view.lines = [];
      else if (name === "k" || name === "ArrowUp") view.following = false;
      else if (name === "G" || name === "f") {
        view.following = true;
        view.unseen = 0;
      }
    } else if (view.exit !== undefined) {
      if (name === "r") runCommand(view.label, view.command);
      else if (name === "Enter" || name === "x") prompt.value = { label: view.label, input: "" };
    }
    return true;
  }

  /** Keys that act on the selected row of the view on screen. */
  function tabKey(name: string): boolean {
    switch (tab.value) {
      case "environments":
        if (name === "Enter") switchTo(ENVIRONMENTS[scroll.value]!.name);
        else if (name === "r") notice.value = { tone: "ok", text: "Checked 2 other environments." };
        else return false;
        return true;
      case "services":
        if (!selectedRow.value) return false;
        if (name === "Enter") {
          const row = selectedRow.value;
          menu.value = {
            label: rowLabel(row),
            service: row.service.name,
            selected: 0,
          };
          return true;
        }
        if (name === " ") {
          const service = selectedRow.value.service;
          if (!foldable(service)) return true;
          const parent = serviceRows.value.findIndex(
            (row) => row.kind === "service" && row.service.name === service.name,
          );
          collapsed.value = collapsed.value.includes(service.name)
            ? collapsed.value.filter((name) => name !== service.name)
            : [...collapsed.value, service.name];
          scroll.value = parent;
          return true;
        }
        return act(name);
      case "variables":
        if (name === "Escape" && filter.value) {
          filter.value = "";
          scroll.value = 0;
          return true;
        }
        return false;
      case "access":
        if (name !== "a") return false;
        openEditor();
        return true;
      default:
        return false;
    }
  }

  /** Handles a key the way the real interface would. Returns whether it was used. */
  function key(name: string): boolean {
    if (confirm.value) {
      if (name === "Enter" || name === "y") {
        const action = confirm.value.action;
        confirm.value = undefined;
        action();
      } else if (name === "Escape" || name === "n") confirm.value = undefined;
      else return false;
      return true;
    }
    if (review.value?.shown) return reviewKey(name);
    if (switching.value) {
      if (name !== "Escape") return true;
      clearTimers();
      sequence += 1;
      switching.value = undefined;
      notice.value = { tone: "info", text: "Switch cancelled." };
      return true;
    }
    if (editor.value) return editorKey(name);
    if (prompt.value) {
      const line = prompt.value;
      if (name === "Escape") prompt.value = undefined;
      else if (name === "Enter") {
        if (line.input.trim()) runCommand(line.label, line.input.trim());
      } else if (name === "ArrowUp") line.input = "bin/rails db:migrate:status";
      else if (name === "Backspace") line.input = line.input.slice(0, -1);
      else if (name.length === 1 && line.input.length < 60) line.input += name;
      return true;
    }
    if (help.value) {
      help.value = false;
      return true;
    }
    if (notice.value) notice.value = undefined;
    if (menu.value) {
      const open = menu.value;
      const count = menuItems.value.length;
      if (name === "Escape" || name === "q") menu.value = undefined;
      else if (name === "j" || name === "ArrowDown") open.selected = (open.selected + 1) % count;
      else if (name === "k" || name === "ArrowUp") open.selected = (open.selected + count - 1) % count;
      else if (name === "Enter") act(menuItems.value[open.selected]!.key);
      else act(name);
      return true;
    }
    if (panel.value) return panelKey(name);
    if (tabKey(name)) return true;
    const digit = DIGITS.indexOf(name);
    if (digit >= 0) {
      open(TABS[digit]!);
      return true;
    }
    const index = TABS.indexOf(tab.value);
    switch (name) {
      case "Tab":
      case "l":
        open(TABS[(index + 1) % TABS.length]!);
        return true;
      case "h":
        open(TABS[(index + TABS.length - 1) % TABS.length]!);
        return true;
      case "]":
      case "[": {
        const group = TAB_GROUPS.findIndex((candidate) => candidate.tabs.includes(tab.value));
        const step = name === "]" ? 1 : TAB_GROUPS.length - 1;
        open(TAB_GROUPS[(group + step) % TAB_GROUPS.length]!.tabs[0]!);
        return true;
      }
      case "j":
      case "ArrowDown":
        move(1);
        return true;
      case "k":
      case "ArrowUp":
        move(-1);
        return true;
      case "?":
        help.value = true;
        return true;
      case "w":
        open("guided");
        return true;
      case "u":
        if (review.value?.stage === "applying") {
          review.value.shown = true;
          return true;
        }
        if (["overview", "services", "variables"].includes(tab.value)) {
          review.value = undefined;
          clearTimers();
          sequence += 1;
          if (busy.value) {
            // A rollout that was interrupted settles before the next review.
            if (remote.value) allUp();
            busy.value = undefined;
          }
          start();
          return true;
        }
        return false;
      case "d":
        if (!["overview", "services", "variables"].includes(tab.value)) return false;
        if (remote.value) {
          notice.value = {
            tone: "info",
            text: `Stopping is not available on remote targets; stop the stack on '${remote.value}' itself.`,
          };
          return true;
        }
        if (stopped.value) return false;
        clearTimers();
        sequence += 1;
        busy.value = undefined;
        confirm.value = {
          title: "Stop the stack?",
          lines: [
            "Removes the containers of app on target 'local'.",
            "Volumes, routes, overrides, and secrets are kept; press u to start again.",
          ],
          accept: "stop",
          action: stop,
        };
        return true;
      default:
        return false;
    }
  }

  /** A key or click from a person. */
  function interact(name: string): boolean {
    const wasTouring = autoplay.value;
    if (wasTouring) {
      clearTimers();
      sequence += 1;
      // Never strand the stack half started, or a switch half done, when someone interrupts.
      if (busy.value) {
        allUp();
        busy.value = undefined;
      }
      switching.value = undefined;
      restarting.value = undefined;
      if (review.value && review.value.stage !== "reviewing") review.value = undefined;
    }
    takeOver();
    const used = key(name);
    if (used) flash(name);
    return used;
  }

  function select(next: TabId) {
    interact("");
    if (menu.value || panel.value || prompt.value || editor.value) return;
    open(next);
  }

  function selectRow(index: number) {
    interact("");
    scroll.value = index;
  }

  onMounted(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      // No tour: show the cluster as deployed and leave the controls to the visitor.
      open("services");
      autoplay.value = false;
      return;
    }
    tour();
  });

  onBeforeUnmount(() => {
    clearTimers();
    clearTimeout(resume);
    stopStream();
  });

  return {
    project: PROJECT,
    tab,
    scroll,
    busy,
    confirm,
    review,
    help,
    notice,
    lastKey,
    autoplay,
    services: present,
    serviceRows,
    runningCount,
    allRunning,
    stopped,
    vitality,
    routes,
    routed,
    variables,
    filter,
    diagnostics,
    issues,
    hints,
    typing,
    environments: ENVIRONMENTS,
    environment,
    current,
    remote,
    nodes,
    deployed,
    local,
    switching,
    menu,
    menuItems,
    panel,
    prompt,
    restarting,
    team,
    accessHere,
    editor,
    editorFields,
    editorDiff,
    interact,
    select,
    selectRow,
  };
}

/** The containers of a service that run and pass their health check: the count the TUI shows. */
export function ready(service: DemoService): number {
  return service.running - (service.replacing ? 1 : 0);
}

/** Whether a service has rows beneath it: its containers, or on a cluster its nodes. */
export function foldable(service: DemoService): boolean {
  return service.nodes ? service.nodes.length > 1 : service.desired > 1;
}

/** HTTPS routes an environment serves: its `ingress:` routes, or on this machine the ones mm makes. */
export function routeCount(environment: DemoEnvironment): number {
  return environment.domains
    ? Object.keys(environment.domains).length
    : ROUTES.filter((route) => route.url).length;
}

/** Whether an environment runs on other machines: one host, or the nodes of a cluster. */
export function isRemote(environment: DemoEnvironment): boolean {
  return Boolean(environment.host || environment.nodes);
}

function plural(count: number, noun: string): string {
  return count === 1 ? `1 ${noun}` : `${count} ${noun}s`;
}

export type TuiDemo = ReturnType<typeof useTuiDemo>;
