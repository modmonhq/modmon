<script setup lang="ts">
import type { Tone } from "@mm/ui/types";
import {
  foldable,
  isRemote,
  LEVELS,
  ready,
  routeCount,
  TAB_GROUPS,
  TAB_TITLES,
  TABS,
  tabDigit,
  type DemoEnvironment,
  type DemoReviewNode,
  type DemoService,
  type ServiceRow,
  type TabId,
  type TuiDemo,
} from "~/composables/useTuiDemo";

/*
 * The mm terminal interface, drawn with the DOM on a fixed column grid. Layout, wording
 * and key hints follow core/src/presentation/tui/view; keep them in step.
 */
const props = defineProps<{ demo: TuiDemo }>();
const {
  project,
  tab,
  scroll,
  busy,
  confirm,
  review,
  help,
  notice,
  services,
  serviceRows,
  runningCount,
  allRunning,
  stopped,
  routes,
  routed,
  variables,
  filter,
  diagnostics,
  issues,
  hints,
  typing,
  environments,
  environment,
  current,
  remote,
  deployed,
  local,
  switching,
  menu,
  menuItems,
  panel,
  prompt,
  restarting,
  team,
  editor,
  editorFields,
  editorDiff,
} = props.demo;

const plural = (count: number, noun: string) => (count === 1 ? `1 ${noun}` : `${count} ${noun}s`);

/** A service as a whole, in the words of `mm service status`. */
function serviceState(service: DemoService): { label: string; tone: Tone } {
  if (service.replacing && service.running === service.desired) return { label: "starting", tone: "warn" };
  if (service.running === service.desired) return { label: "running", tone: "ok" };
  if (service.running > 0) return { label: "degraded", tone: "warn" };
  return { label: "missing", tone: "idle" };
}

/** What follows a service's name: its module, or on a cluster where it runs. */
function serviceLabel(service: DemoService): string {
  if (!service.nodes) return service.module;
  return service.nodes.length === 1 ? service.nodes[0]! : plural(service.nodes.length, "node");
}

function uptime(row: Extract<ServiceRow, { kind: "container" }>): string {
  return row.state === "exited" ? "Exited (137)" : "Up 2 hours";
}

/**
 * A node's row pads the node name to the widest node of any service, and the health word to the
 * widest of its own service, so the columns line up as they do in the TUI.
 */
const nodeWidth = computed(() =>
  Math.max(0, ...services.value.flatMap((service) => service.nodes ?? []).map((node) => node.length)),
);
function nodeColumns(row: Extract<ServiceRow, { kind: "node" }>): { node: string; health: string } {
  const widest = Math.max(
    ...serviceRows.value.map((other) =>
      other.kind === "node" && other.service === row.service ? other.health.length : 0,
    ),
  );
  return { node: row.node.padEnd(nodeWidth.value), health: row.health.padEnd(widest) };
}

/** The review's question, or what it shows while and after the plan is applied. */
const reviewTitle = computed(() => {
  const open = review.value;
  if (!open) return "";
  if (open.stage === "applied") return `Plan ${open.plan}`;
  if (open.stage === "applying")
    return open.remote ? `Deploying to ${environment.value}` : `Starting ${environment.value}`;
  return open.remote ? `Deploy to ${environment.value}?` : `Start ${environment.value}?`;
});
const planIntro = "The plan as `mm deploy plan` prints it:";
const reviewNodeWidth = computed(() =>
  Math.max(0, ...(review.value?.nodes ?? []).map((row) => row.node.length)),
);

/** One node's row while the plan is applied: its mark, tone, and what it is doing. */
function reviewNode(row: DemoReviewNode): { glyph: string; tone: string; text: string } {
  if (row.stage === "done") return { glyph: "●", tone: "t-accent", text: "done" };
  if (row.stage === "working") return { glyph: "", tone: "t-info", text: row.step ?? "" };
  return row.stacks.length
    ? { glyph: "◐", tone: "t-info", text: "waits for its next turn" }
    : { glyph: "○", tone: "t-muted", text: "waiting" };
}

/** Each node keeps one color in a merged log, so a line's origin reads at a glance. */
const sourceTones = ["t-accent", "t-info", "t-warn"];
function sourceTone(sources: string[], node: string): string {
  return sourceTones[sources.indexOf(node) % sourceTones.length] ?? "t-muted";
}

const variableTone: Record<string, Tone> = { resolved: "ok", defaulted: "info", unset: "bad" };

function tabMeta(id: TabId): { text: string; tone: Tone } | undefined {
  switch (id) {
    case "environments":
      return { text: String(environments.length), tone: "idle" };
    case "services":
      return services.value.length
        ? {
            text: `${runningCount.value}/${services.value.length}`,
            tone: allRunning.value ? "ok" : "warn",
          }
        : undefined;
    case "ingress":
      return routed.value ? { text: `${routed.value} ↗`, tone: "idle" } : undefined;
    case "diagnostics":
      return issues.value.length
        ? { text: String(issues.value.length), tone: "warn" }
        : { text: "✓", tone: "ok" };
    case "tls":
      return { text: "✓", tone: "ok" };
    case "gates":
      return { text: "2", tone: "idle" };
    case "jobs":
      return { text: "3", tone: "idle" };
    default:
      return undefined;
  }
}

const selectedVariable = computed(() => variables.value[scroll.value] ?? variables.value[0]);
const variableAction = computed(() => {
  const variable = selectedVariable.value;
  if (!variable?.key) return "edit the module template to change it";
  if (remote.value)
    return "enter sets it in the configuration for this environment; commit and deploy to apply";
  if (variable.sensitive) return "enter sets the secret (hidden input)";
  if (variable.source === "local override") {
    return "enter edits the override · x removes it · c sets it in the configuration";
  }
  return "enter sets a local override · c sets it in the configuration";
});

/** What the Environments view knows about one environment. */
function environmentStatus(entry: DemoEnvironment): { text: string; tone: Tone } {
  if (entry.name !== environment.value && !isRemote(entry)) {
    // The stack on this machine keeps running while another environment is on screen.
    const up = local.value.filter((service) => service.present);
    const running = up.filter((service) => service.running === service.desired).length;
    return up.length
      ? { text: `${running}/${up.length} running`, tone: running === up.length ? "ok" : "warn" }
      : { text: "nothing running", tone: "idle" };
  }
  if (entry.name !== environment.value) return { text: "5/5 running", tone: "ok" };
  return services.value.length ? statusOf() : { text: "nothing running", tone: "idle" };
}
function statusOf(): { text: string; tone: Tone } {
  return {
    text: `${runningCount.value}/${services.value.length} running`,
    tone: allRunning.value ? "ok" : "warn",
  };
}
const selectedEnvironment = computed(() => environments[scroll.value] ?? environments[0]!);
const switchingTo = computed(() => environments.find((entry) => entry.name === switching.value));
const place = computed(() => (remote.value ? `remote · ${remote.value}` : "local"));
const you = computed(() => team.value.find((member) => member.you));

/** The form's label for one of its fields. */
function fieldLabel(field: string): string {
  if (field === "name") return "Name *";
  return field === "key" ? "Public key *" : field;
}

const helpRows: [string, string][] = [
  ["1-9, 0", "open view by number"],
  ["2, enter", "switch environment"],
  ["[ ]", "previous / next group"],
  ["tab, h/l", "next / previous view"],
  ["j/k, ↑↓", "move selection"],
  ["pgup/pgdn", "move by page"],
  ["home/end", "first / last row"],
  ["ctrl-r", "refresh status now"],
];

function onKeydown(event: KeyboardEvent) {
  // Tab keeps moving focus through the page; every other plain key drives the interface.
  if (event.metaKey || event.ctrlKey || event.altKey || event.key === "Tab") return;
  if (props.demo.interact(event.key)) event.preventDefault();
}
</script>

<template>
  <div
    class="tui"
    tabindex="0"
    role="group"
    aria-label="Interactive preview of the mm terminal interface. Number keys switch views."
    @keydown="onKeydown"
  >
    <!-- Header -->
    <div class="tui-header" :class="{ 'is-busy': busy }">
      <div class="tui-identity">
        <span class="tui-brand">mm</span>
        <b class="t-strong">{{ project }}</b>
        <span class="tui-chip" :class="{ 'tui-chip--remote': remote }">{{ environment }}</span>
        <span class="tui-tag" :class="{ 't-info': remote }">{{ place }}</span>
        <span v-if="!(busy && remote)" class="tui-tag tui-tag--wide"><i>·</i>{{ current.runtime }}</span>
      </div>
      <div class="tui-status">
        <span v-if="busy" class="t-accent tui-busy"
          ><MmSpinner /><span class="g1">{{ busy }}…</span></span
        >
        <MmBadge v-if="services.length" :tone="allRunning ? 'ok' : 'warn'" class="tui-status__count">
          {{ runningCount }}/{{ services.length }} running
        </MmBadge>
        <MmBadge v-if="issues.length" tone="warn">{{ plural(issues.length, "issue") }}</MmBadge>
        <MmBadge v-else tone="ok" class="tui-status__issues">no issues</MmBadge>
        <MmBadge tone="ok" pulse>live</MmBadge>
      </div>
    </div>

    <!-- A log or command view takes the whole width, sidebar included. -->
    <div v-if="panel || review?.shown" class="tui-body tui-body--panel">
      <!-- The review of a plan, and where its apply is, as core/src/presentation/tui/view/review.rs draws it. -->
      <MmCard v-if="review?.shown" :title="reviewTitle" focused class="tui-grow tui-output tui-review">
        <template #meta>
          <MmHints
            v-if="review.stage === 'reviewing'"
            :items="[
              ['enter', review.remote ? 'deploy' : 'start'],
              ['esc', 'cancel'],
            ]"
          />
          <span v-else-if="review.stage === 'applying'" class="t-accent tui-busy"
            ><MmSpinner /><span class="g1"
              >{{ busy ?? "Applying" }}<span class="hide-sm"> plan {{ review.plan }}</span></span
            ></span
          >
          <MmBadge v-else tone="ok">applied</MmBadge>
        </template>
        <div class="tui-review__head">
          <div class="tui-review__wrap">
            {{ review.head
            }}<span class="t-muted">{{ `   ${plural(review.routes, "route")} · plan ${review.plan}` }}</span>
          </div>
          <template v-if="review.stage !== 'reviewing' && review.nodes.length">
            <div class="l" />
            <div v-for="row in review.nodes" :key="row.node" class="l i2">
              <span :class="reviewNode(row).tone"
                ><MmSpinner v-if="row.stage === 'working'" /><template v-else>{{
                  reviewNode(row).glyph
                }}</template></span
              ><b class="t-strong g1">{{ row.node.padEnd(reviewNodeWidth) }}</b
              ><span class="g2" :class="reviewNode(row).tone">{{ reviewNode(row).text }}</span
              ><span v-if="row.stacks.length" class="t-muted hide-sm">{{
                `   stacks applied: ${row.stacks.join(", ")}`
              }}</span>
            </div>
          </template>
          <div v-if="review.stage === 'reviewing'" class="l t-muted">{{ planIntro }}</div>
          <template v-else-if="review.stage === 'applied'">
            <div class="l" />
            <div class="l">
              <MmBadge tone="ok">{{ review.summary }}</MmBadge>
            </div>
          </template>
        </div>
        <div class="tui-review__plan">
          <div class="tui-review__rows">
            <div
              v-for="(line, index) in review.lines.slice(review.scroll)"
              :key="review.scroll + index"
              class="tui-review__line"
              :class="{ 't-strong': line && !line.startsWith(' ') }"
            >
              {{ line }}
            </div>
          </div>
        </div>
      </MmCard>
      <MmCard v-else-if="panel?.kind === 'logs'" focused class="tui-grow tui-output">
        <template #title
          >Logs · {{ panel.label
          }}<span v-if="panel.where" class="t-muted tui-title-note g2">{{ panel.where }}</span
          ><span v-else-if="panel.nodes.length > 1" class="t-muted tui-title-note g2"
            >{{ panel.nodes.length }} nodes, in order</span
          ></template
        >
        <template #meta>
          <MmBadge v-if="panel.following" tone="ok">following</MmBadge>
          <MmBadge v-else tone="warn">{{ panel.unseen ? `paused · ${panel.unseen} new` : "paused" }}</MmBadge>
          <span class="g3 hide-sm">{{ plural(panel.lines.length, "line") }}</span>
          <span class="g3 hide-sm">{{ environment }} · {{ place }}</span>
        </template>
        <div class="tui-output__lines">
          <div
            v-for="(line, index) in panel.lines"
            :key="index"
            class="l tui-line"
            :class="{ 'is-stderr': line.stderr }"
            :data-tone="line.tone"
          >
            <span v-if="panel.times" class="t-muted tui-line__time">{{ line.time }}</span
            ><template v-if="line.node"
              ><span :class="sourceTone(panel.nodes, line.node)">{{ line.node.padEnd(6) }}</span
              ><span class="t-muted hide-sm">{{ line.container }}</span
              ><span class="t-border tui-line__rule">│</span></template
            ><span>{{ line.text }}</span>
          </div>
        </div>
      </MmCard>
      <MmCard v-else-if="panel" :title="`Run · ${panel.label}`" focused class="tui-grow tui-output">
        <template #meta>
          <span v-if="panel.exit === undefined" class="t-accent tui-busy"
            ><MmSpinner /><span class="g1">running</span></span
          >
          <MmBadge v-else tone="ok">exit {{ panel.exit }}</MmBadge>
          <span class="g3 hide-sm">{{ environment }} · {{ place }}</span>
        </template>
        <div class="tui-search">
          <b class="t-key">$</b><b class="t-strong g1">{{ panel.command }}</b>
        </div>
        <div class="tui-output__lines">
          <div
            v-for="(line, index) in panel.lines"
            :key="index"
            class="l tui-line"
            :class="{ 'is-stderr': line.stderr }"
            :data-tone="line.tone"
          >
            {{ line.text }}
          </div>
        </div>
      </MmCard>
    </div>

    <div v-else class="tui-body">
      <!-- Sidebar (wide terminals) -->
      <div class="tui-sidebar">
        <div v-for="group in TAB_GROUPS" :key="group.title" class="tui-group">
          <div class="tui-group__title">{{ group.title }}</div>
          <button
            v-for="id in group.tabs"
            :key="id"
            type="button"
            tabindex="-1"
            class="tui-nav"
            :class="{ 'is-selected': id === tab }"
            :aria-current="id === tab ? 'true' : undefined"
            @click="demo.select(id)"
          >
            <span class="tui-nav__digit">{{ tabDigit(id) ?? "·" }}</span>
            <span class="tui-nav__title">{{ TAB_TITLES[id] }}</span>
            <span v-if="tabMeta(id)" class="tui-nav__meta" :data-tone="tabMeta(id)!.tone">
              {{ tabMeta(id)!.text }}
            </span>
          </button>
        </div>
      </div>

      <div class="tui-main">
        <!-- Tab strip (narrow terminals) -->
        <button type="button" tabindex="-1" class="tui-strip" @click="demo.interact('l')">
          <span class="tui-strip__group">{{
            TAB_GROUPS.find((group) => group.tabs.includes(tab))!.title
          }}</span>
          <span class="tui-strip__tab">{{ TAB_TITLES[tab] }}</span>
          <span class="t-muted"
            >{{ TABS.indexOf(tab) + 1 }}/{{ TABS.length }}<span class="g3">tab next</span></span
          >
        </button>

        <div class="tui-content" :data-view="tab">
          <!-- Overview -->
          <template v-if="tab === 'overview'">
            <div class="tui-stats">
              <MmCard title="Services">
                <div class="l">
                  <MmTone :tone="stopped ? 'idle' : allRunning ? 'ok' : 'warn'" />
                  <b class="t-strong g1">{{ runningCount }}/{{ services.length }}</b>
                </div>
                <div class="l t-muted">running</div>
              </MmCard>
              <MmCard title="Routes">
                <div class="l">
                  <MmTone :tone="routed ? 'ok' : 'idle'" /><b class="t-strong g1">{{ routed }}</b>
                </div>
                <div class="l t-muted">of {{ routes.length }} services routed</div>
              </MmCard>
              <MmCard title="Issues">
                <div class="l">
                  <MmTone :tone="issues.length ? 'warn' : 'ok'" />
                  <b class="t-strong g1">{{ issues.length }}</b>
                </div>
                <div class="l t-muted">
                  {{ issues.length ? `0 errors · ${plural(issues.length, "warning")}` : "all checks passed" }}
                </div>
              </MmCard>
              <MmCard title="HTTPS">
                <div class="l"><MmTone tone="ok" /><b class="t-strong g1">trusted</b></div>
                <div class="l t-muted">2 hosts</div>
              </MmCard>
            </div>
            <div class="tui-split">
              <MmCard title="Workspace" class="hide-sm">
                <template #meta><MmHints :items="[['2', 'environments']]" /></template>
                <div class="l kv"><span>Root</span>~/code/{{ project }}</div>
                <div class="l kv">
                  <span>Runs on</span>
                  <MmBadge v-if="remote" tone="info">remote · {{ remote }}</MmBadge>
                  <template v-else>this machine</template>
                </div>
                <div class="l kv"><span>Config</span><MmBadge tone="ok">valid</MmBadge></div>
                <div class="l kv"><span>Runtime</span>{{ current.runtime }}</div>
                <div class="l kv"><span>Engine</span>28.1.1</div>
                <div class="l kv"><span>Env delivery</span>env-file · development-default</div>
                <div class="l kv"><span>Environments</span>{{ environments.length }} · 2 remote</div>
              </MmCard>
              <MmCard title="Needs attention">
                <template v-if="issues.length" #meta><MmHints :items="[['7', 'details']]" /></template>
                <template v-if="stopped">
                  <div class="l"><MmBadge tone="idle">The stack is not running</MmBadge></div>
                  <div class="l t-muted i2"><b class="t-key">u</b> reviews and starts it</div>
                </template>
                <template v-else-if="issues.length">
                  <div v-for="issue in issues" :key="issue.code" class="l">
                    <MmBadge :tone="issue.tone">{{ issue.message }}</MmBadge>
                  </div>
                </template>
                <div v-else class="tui-empty"><MmBadge tone="ok">Everything looks good</MmBadge></div>
              </MmCard>
            </div>
          </template>

          <!-- Environments -->
          <template v-else-if="tab === 'environments'">
            <div class="tui-pair tui-grow">
              <MmCard title="Environments" focused>
                <template #meta>{{ environments.length }} environments · 2 remote</template>
                <div class="tt tt--environments">
                  <div class="tr th"><span>ENVIRONMENT</span><span>RUNS ON</span><span>SERVICES</span></div>
                  <div
                    v-for="(entry, index) in environments"
                    :key="entry.name"
                    class="tr"
                    :class="{ 'is-selected': index === scroll }"
                    @click="demo.selectRow(index)"
                  >
                    <span>
                      <b :class="entry.name === environment ? 't-key' : 't-strong'">{{ entry.name }}</b>
                      <span v-if="entry.name === environment" class="t-accent g2">current</span>
                      <span v-else-if="entry.isDefault" class="t-muted g2">default</span>
                    </span>
                    <span>
                      <MmBadge v-if="isRemote(entry)" tone="info">{{ entry.target }}</MmBadge>
                      <MmBadge v-else tone="idle">local</MmBadge>
                    </span>
                    <span>
                      <MmBadge :tone="environmentStatus(entry).tone">{{
                        environmentStatus(entry).text
                      }}</MmBadge>
                    </span>
                  </div>
                </div>
              </MmCard>
              <MmCard :title="selectedEnvironment.name" class="hide-sm">
                <div class="l">
                  <MmBadge v-if="isRemote(selectedEnvironment)" tone="info"
                    >remote · {{ selectedEnvironment.target }}</MmBadge
                  >
                  <MmBadge v-else tone="idle">local · this machine</MmBadge>
                </div>
                <div class="l" />
                <div v-if="selectedEnvironment.host" class="l kv">
                  <span>Host</span>{{ selectedEnvironment.host }}
                </div>
                <div class="l kv"><span>Runtime</span>{{ selectedEnvironment.runtime }}</div>
                <div v-if="selectedEnvironment.deployFrom" class="l kv">
                  <span>Deploys from</span>{{ selectedEnvironment.deployFrom }}
                </div>
                <template v-if="isRemote(selectedEnvironment)">
                  <div class="l kv">
                    <span>You are</span><b class="t-strong">alice</b>
                    <span class="g2">{{ you?.access[selectedEnvironment.name] ?? "no access here" }}</span>
                  </div>
                  <div class="l kv"><span>Deployed</span>{{ deployed[selectedEnvironment.name] }}</div>
                </template>
                <div class="l kv"><span>Routes</span>{{ routeCount(selectedEnvironment) }}</div>
                <template v-if="selectedEnvironment.nodes">
                  <div class="l" />
                  <div class="l t-section">Nodes</div>
                  <!-- As the status read finds each node, before the node check adds mmd and the mesh. -->
                  <div v-for="node in selectedEnvironment.nodes" :key="node.name" class="l">
                    <MmBadge tone="ok">{{ node.name }}</MmBadge>
                    <span class="t-muted g2">answering</span>
                    <span class="t-muted g2">{{ deployed[selectedEnvironment.name] }}</span>
                  </div>
                </template>
                <div class="l" />
                <div class="l">
                  <MmHints
                    v-if="selectedEnvironment.name !== environment"
                    :items="[
                      ['enter', 'switch'],
                      ['r', 'check again'],
                    ]"
                  />
                </div>
                <div v-if="selectedEnvironment.name === environment" class="t-muted tui-wrap">
                  {{ "Every other view shows this environment." }}
                </div>
              </MmCard>
            </div>
          </template>

          <!-- Services -->
          <template v-else-if="tab === 'services'">
            <MmCard v-if="remote" title="Target" class="tui-fixed">
              <template #meta><MmHints :items="[['9', 'access']]" /></template>
              <div class="l">
                <MmBadge tone="info">remote · {{ remote }}</MmBadge>
                <span class="t-muted g3">{{ current.runtime }}</span>
              </div>
              <div class="l">
                <span class="t-muted">you are</span><b class="t-strong g1">alice</b>
                <span class="g2">{{ you?.access[environment] ?? "no access here" }}</span>
                <span class="t-muted g3 hide-sm">deployed {{ deployed[environment] }}</span>
              </div>
            </MmCard>
            <MmCard v-else title="Control plane" class="tui-fixed">
              <template #meta><MmBadge tone="ok">installed</MmBadge></template>
              <div class="l">
                <MmBadge tone="ok">ingress</MmBadge><span class="t-muted g1">3.4</span>
                <MmBadge tone="ok" class="g4">control api</MmBadge><span class="t-muted g1">1.0</span>
                <MmBadge tone="ok" class="g4">mailpit</MmBadge><span class="t-muted g1">1.21</span>
              </div>
              <div class="l">
                <span class="t-accent">https://mm.test</span>
                <span class="t-muted g3">smtp mailpit:1025 → 127.0.0.1:1025</span>
                <span class="t-muted g3 hide-sm">docker compose</span>
              </div>
            </MmCard>
            <MmCard title="Services" focused class="tui-grow">
              <template #meta>
                {{ plural(services.length, "service") }}
                <MmHints v-if="services.length" :items="[['enter', 'actions']]" class="g3" />
              </template>
              <div v-if="!services.length" class="tui-empty tui-empty--detail">
                <MmBadge tone="idle">No services running</MmBadge>
                <span class="t-muted">Press u to review and start the stack.</span>
              </div>
              <div v-else class="tt tt--services">
                <div class="tr th">
                  <span>STATE</span><span>SERVICE</span><span>REPLICAS</span>
                  <span class="hide-sm">RESTARTS</span>
                </div>
                <div
                  v-for="(row, index) in serviceRows"
                  :key="
                    row.kind === 'service'
                      ? row.service.name
                      : row.kind === 'node'
                        ? `${row.service.name}@${row.node}`
                        : row.name
                  "
                  class="tr"
                  :class="{ 'is-selected': index === scroll, 'tr--node': row.kind === 'node' }"
                  @click="demo.selectRow(index)"
                >
                  <template v-if="row.kind === 'service'">
                    <span>
                      <span v-if="restarting === row.service.name" class="t-info tui-busy"
                        ><MmSpinner /><span class="g1">restarting</span></span
                      >
                      <MmBadge v-else :tone="serviceState(row.service).tone">{{
                        serviceState(row.service).label
                      }}</MmBadge>
                    </span>
                    <span>
                      <span class="t-muted tui-fold">{{
                        foldable(row.service) ? (row.expanded ? "▾" : "▸") : ""
                      }}</span>
                      <b class="t-strong">{{ row.service.name }}</b>
                      <span class="t-muted g2">{{ serviceLabel(row.service) }}</span>
                    </span>
                    <span>
                      <MmPips
                        :desired="row.service.desired"
                        :running="row.service.running"
                        :healthy="ready(row.service)"
                      />
                      <span class="t-muted g2">{{ ready(row.service) }}/{{ row.service.desired }}</span>
                    </span>
                    <span class="hide-sm" :class="row.service.restarts ? 't-warn' : 't-muted'">
                      {{ row.service.restarts }}
                    </span>
                  </template>
                  <template v-else-if="row.kind === 'node'">
                    <span />
                    <span class="tui-node">
                      <span class="t-border tui-branch">{{ row.last ? "└" : "├" }}</span>
                      <span class="t-soft">{{ nodeColumns(row).node }}</span>
                      <span class="t-muted g2">{{ row.container }}</span>
                      <span class="g2" :class="row.health === 'starting' ? 't-warn' : 't-muted'">{{
                        nodeColumns(row).health
                      }}</span>
                      <span class="g2" :class="row.fresh ? 't-accent' : 't-muted'">{{ row.status }}</span>
                    </span>
                  </template>
                  <template v-else>
                    <span class="i1">
                      <MmBadge :tone="row.state === 'running' ? 'ok' : 'bad'">{{ row.state }}</MmBadge>
                    </span>
                    <span>
                      <span class="t-border tui-branch">{{ row.last ? "└" : "├" }}</span>
                      <span class="t-soft">{{ row.name }}</span>
                    </span>
                    <span class="t-muted">{{ uptime(row) }}</span>
                    <span class="t-muted hide-sm">0</span>
                  </template>
                </div>
              </div>
            </MmCard>
          </template>

          <!-- Ingress -->
          <template v-else-if="tab === 'ingress'">
            <MmCard title="Routes" focused class="tui-grow">
              <template #meta>{{ routed }} of {{ routes.length }} routed</template>
              <div v-if="!routes.length" class="tui-empty tui-empty--detail">
                <MmBadge tone="idle">No routable services</MmBadge>
                <span class="t-muted">Services with HTTP ports appear here once configured.</span>
              </div>
              <div v-else class="tt tt--ingress">
                <div class="tr th">
                  <span>SERVICE</span><span>URL</span><span>PORT</span><span class="hide-sm">SOURCE</span>
                </div>
                <div
                  v-for="(route, index) in routes"
                  :key="route.service"
                  class="tr"
                  :class="{ 'is-selected': index === scroll }"
                  @click="demo.selectRow(index)"
                >
                  <span>
                    <b class="t-strong">{{ route.service }}</b>
                    <span class="t-muted g2">{{ route.stack }}</span>
                  </span>
                  <span v-if="route.url"
                    ><span class="t-accent">↗</span><span class="g1">{{ route.url }}</span></span
                  >
                  <span v-else class="t-muted"
                    >·<span class="g1">{{ route.reason }}</span></span
                  >
                  <span class="t-soft">{{ route.port ?? "—" }}</span>
                  <span class="t-muted hide-sm">{{ route.source }}</span>
                </div>
              </div>
            </MmCard>
          </template>

          <!-- Variables -->
          <template v-else-if="tab === 'variables'">
            <MmCard title="Variables" focused class="tui-grow">
              <template #meta>
                {{ plural(variables.length, "variable") }}
                <MmBadge v-if="remote" tone="info" class="g2">from this checkout</MmBadge>
                <span class="g2 hide-sm">env-file delivery</span>
              </template>
              <div class="tui-search">
                <span v-if="filter"
                  ><span class="t-muted">/</span><span class="g1">{{ filter }}</span></span
                >
                <span v-else class="t-muted"
                  >/<span class="g1">search name, key, service, value, or status</span></span
                >
              </div>
              <div class="tt tt--environment">
                <div class="tr th">
                  <span>SERVICE</span><span>VARIABLE</span><span>STATUS</span>
                  <span class="hide-sm">SOURCE</span><span class="hide-sm">VALUE</span>
                </div>
                <div
                  v-for="(variable, index) in variables"
                  :key="variable.service + variable.name"
                  class="tr"
                  :class="{ 'is-selected': index === scroll }"
                  @click="demo.selectRow(index)"
                >
                  <span>
                    <template v-if="variable.first">
                      <b class="t-strong">{{ variable.service }}</b>
                      <span class="t-muted g2">{{ variable.stack }}</span>
                    </template>
                  </span>
                  <span>{{ variable.name }}</span>
                  <span>
                    <MmBadge :tone="variableTone[variable.status]!">{{ variable.status }}</MmBadge>
                  </span>
                  <span
                    class="hide-sm"
                    :class="variable.source === 'local override' ? 't-accent' : 't-muted'"
                  >
                    {{ variable.source }}
                  </span>
                  <span v-if="variable.sensitive" class="t-muted hide-sm">••••••</span>
                  <span v-else class="hide-sm">{{ variable.value ?? "—" }}</span>
                </div>
              </div>
              <div v-if="selectedVariable" class="tui-detail">
                <div class="tui-detail__title">
                  <b class="t-strong">{{ selectedVariable.name }} · {{ selectedVariable.service }}</b>
                </div>
                <div class="l kv kv--10">
                  <span>Value</span>
                  <span v-if="selectedVariable.sensitive" class="t-muted">••••••</span>
                  <template v-else>{{ selectedVariable.value ?? "—" }}</template>
                </div>
                <div class="l kv kv--10">
                  <span>Key</span>{{ selectedVariable.key ?? "none (template only)" }}
                  <span class="t-muted g2">required</span>
                </div>
                <div class="l kv kv--10">
                  <span>Template</span
                  >{{ selectedVariable.service === "web" ? "frontend" : "backend" }}/app.env:{{
                    selectedVariable.line
                  }}
                </div>
                <div class="l t-muted">{{ variableAction }}</div>
              </div>
            </MmCard>
          </template>

          <!-- Mounts -->
          <template v-else-if="tab === 'mounts'">
            <MmCard title="Development mounts" focused class="tui-grow">
              <template #meta>1 of 2 configured</template>
              <div class="tt tt--mounts">
                <div class="tr th">
                  <span>MODULE</span><span>SOURCE → CONTAINER</span><span class="hide-sm">CONFIDENCE</span>
                  <span>STATE</span>
                </div>
                <div class="tr" :class="{ 'is-selected': scroll === 0 }" @click="demo.selectRow(0)">
                  <b class="t-strong">frontend</b>
                  <span
                    >frontend/src<span class="t-accent g1">→</span
                    ><span class="t-soft g1">/app/src</span></span
                  >
                  <span class="t-accent hide-sm">high</span>
                  <span><MmBadge tone="ok">configured</MmBadge></span>
                </div>
                <div class="tr" :class="{ 'is-selected': scroll === 1 }" @click="demo.selectRow(1)">
                  <b class="t-strong">backend</b>
                  <span
                    >backend/src<span class="t-accent g1">→</span
                    ><span class="t-soft g1">/srv/app/src</span></span
                  >
                  <span class="t-info hide-sm">medium</span>
                  <span><MmBadge tone="idle">proposal</MmBadge></span>
                </div>
              </div>
            </MmCard>
          </template>

          <!-- Diagnostics -->
          <template v-else-if="tab === 'diagnostics'">
            <MmCard title="Diagnostics" focused class="tui-grow">
              <template #meta>{{ plural(diagnostics.length, "finding") }}</template>
              <div
                v-for="(item, index) in diagnostics"
                :key="item.code"
                class="tui-finding"
                :class="{ 'is-selected': index === scroll }"
                :data-tone="item.tone"
                @click="demo.selectRow(index)"
              >
                <div class="l">
                  <MmTone :tone="item.tone" /><b class="t-strong g1">{{ item.message }}</b>
                </div>
                <div class="l t-tone i2">{{ item.code }}</div>
                <div v-if="item.help" class="l t-soft i2">{{ item.help }}</div>
              </div>
            </MmCard>
          </template>

          <!-- TLS -->
          <template v-else-if="tab === 'tls'">
            <MmCard title="Local HTTPS" focused class="tui-grow">
              <template #meta><MmBadge tone="ok">ready</MmBadge></template>
              <div class="l"><MmBadge tone="ok">Local CA installed · generation 3</MmBadge></div>
              <div class="l"><MmBadge tone="ok">Private key is private</MmBadge></div>
              <div class="l"><MmBadge tone="ok">Trusted by your user certificate store</MmBadge></div>
              <div class="l"><MmBadge tone="ok">No rotation pending</MmBadge></div>
              <div class="l" />
              <div class="l t-section">Issued hosts</div>
              <div class="l i2">
                <span class="t-accent">↗</span><span class="g1">api.{{ project }}.mm.test</span>
              </div>
              <div class="l i2">
                <span class="t-accent">↗</span><span class="g1">web.{{ project }}.mm.test</span>
              </div>
            </MmCard>
          </template>

          <!-- Access -->
          <template v-else-if="tab === 'access'">
            <MmCard title="Access" class="tui-fixed">
              <div class="l kv"><span>Environment</span>{{ environment }}</div>
              <div class="l kv">
                <span>Runs on</span>
                <MmBadge v-if="remote" tone="info">remote · {{ remote }}</MmBadge>
                <MmBadge v-else tone="idle">local</MmBadge>
              </div>
              <div class="l kv"><span>Your key</span>SHA256:qVx3…k9Fw</div>
              <div v-if="remote" class="l kv">
                <span>You are</span><b class="t-strong">alice</b>
                <span class="g2">{{ you?.access[environment] }}</span>
              </div>
            </MmCard>
            <MmCard title="Team" focused class="tui-grow">
              <template #meta>{{ plural(team.length, "member") }}</template>
              <div class="tt tt--team">
                <div class="tr th">
                  <span>NAME</span><span>ACCESS HERE</span><span class="hide-sm">KEYS</span>
                </div>
                <div
                  v-for="(member, index) in team"
                  :key="member.name"
                  class="tr"
                  :class="{ 'is-selected': index === scroll }"
                  @click="demo.selectRow(index)"
                >
                  <span>
                    <b class="t-strong">{{ member.name }}</b>
                    <span v-if="member.you" class="t-accent g2">you</span>
                  </span>
                  <span v-if="demo.accessHere(member)">{{ demo.accessHere(member) }}</span>
                  <span v-else class="t-muted">no access here</span>
                  <span class="t-muted hide-sm"
                    >{{ plural(member.keys, "key") }}{{ member.machine ? " · automation" : "" }}</span
                  >
                </div>
              </div>
            </MmCard>
          </template>

          <!-- Gates -->
          <template v-else-if="tab === 'gates'">
            <MmCard title="Access gates" focused class="tui-grow">
              <template #meta>2 policies</template>
              <div class="tt tt--gates">
                <div class="tr th">
                  <span>POLICY</span><span>MODE</span><span>SESSION</span><span>REVISION</span>
                </div>
                <div class="tr" :class="{ 'is-selected': scroll === 0 }" @click="demo.selectRow(0)">
                  <b class="t-strong">preview</b><span class="t-soft">any driver</span>
                  <span class="t-soft">7d</span><span class="t-muted">rev 3</span>
                </div>
                <div class="tr" :class="{ 'is-selected': scroll === 1 }" @click="demo.selectRow(1)">
                  <b class="t-strong">admin</b><span class="t-soft">all drivers</span>
                  <span class="t-soft">1d</span><span class="t-muted">rev 1</span>
                </div>
              </div>
            </MmCard>
          </template>

          <!-- Providers -->
          <template v-else-if="tab === 'providers'">
            <MmCard title="Managed provider" class="tui-fixed">
              <div class="l kv kv--12">
                <span>Release</span>2.4.1<b class="t-key g3">[ ]</b><span class="t-muted g1">change</span>
              </div>
              <div class="l kv kv--12">
                <span>Status</span><b class="t-key">s</b>
                <span class="t-muted g1">check health, seal, and recovery</span>
              </div>
            </MmCard>
            <MmCard title="Connections" focused class="tui-grow">
              <div class="tt tt--providers">
                <div class="tr th"><span>KIND</span><span>NAME</span><span>STATUS</span></div>
                <div class="tr" :class="{ 'is-selected': scroll === 0 }" @click="demo.selectRow(0)">
                  <span class="t-muted">secret</span><b class="t-strong">session.secret</b>
                  <span class="t-muted">metadata only · v plans a rotation</span>
                </div>
                <div class="tr" :class="{ 'is-selected': scroll === 1 }" @click="demo.selectRow(1)">
                  <span class="t-muted">secret</span><b class="t-strong">payments.api_key</b>
                  <span class="t-muted">metadata only · v plans a rotation</span>
                </div>
              </div>
            </MmCard>
          </template>

          <!-- Jobs -->
          <template v-else-if="tab === 'jobs'">
            <div class="tui-panes tui-grow">
              <MmCard title="Jobs" focused>
                <div class="tt tt--jobs">
                  <div class="tr" :class="{ 'is-selected': scroll === 0 }" @click="demo.selectRow(0)">
                    <b class="t-strong">migrate</b><span class="t-muted">backend/api</span>
                  </div>
                  <div class="tr" :class="{ 'is-selected': scroll === 1 }" @click="demo.selectRow(1)">
                    <b class="t-strong">cleanup</b><span class="t-muted">backend/worker</span>
                  </div>
                  <div class="tr" :class="{ 'is-selected': scroll === 2 }" @click="demo.selectRow(2)">
                    <b class="t-strong">reindex</b><span class="t-muted">backend/worker</span>
                  </div>
                </div>
              </MmCard>
              <MmCard title="Schedules">
                <div class="tt tt--schedules">
                  <div class="tr">
                    <span><MmBadge tone="ok">on</MmBadge></span
                    ><b class="t-strong">daily</b>
                    <span class="t-muted">Nov 13 02:00</span>
                  </div>
                  <div class="tr">
                    <span><MmBadge tone="idle">off</MmBadge></span
                    ><b class="t-strong">weekly</b>
                    <span class="t-muted">—</span>
                  </div>
                </div>
              </MmCard>
              <MmCard title="Runs">
                <div class="tt tt--runs">
                  <div class="tr">
                    <span class="t-info"><MmSpinner /><span class="g1">running</span></span
                    ><b class="t-strong">cleanup</b>
                    <span class="t-muted">1/1</span>
                  </div>
                  <div class="tr">
                    <span><MmBadge tone="ok">succeeded</MmBadge></span
                    ><b class="t-strong">migrate</b>
                    <span class="t-muted">1/1</span>
                  </div>
                  <div class="tr">
                    <span><MmBadge tone="ok">succeeded</MmBadge></span
                    ><b class="t-strong">cleanup</b>
                    <span class="t-muted">1/1</span>
                  </div>
                </div>
              </MmCard>
            </div>
          </template>

          <!-- Guided -->
          <template v-else>
            <MmCard title="Guided setup" focused class="tui-grow">
              <div class="l tui-stepper">
                <b class="t-info">◉ Choose</b>
                <template v-for="step in ['Discover', 'Configure', 'Review', 'Apply', 'Done']" :key="step">
                  <span class="t-border g1">─</span><span class="t-muted g1">○ {{ step }}</span>
                </template>
              </div>
              <div class="l" />
              <div class="l t-section">Start a journey</div>
              <div
                v-for="[key, label] in [
                  ['g', 'Set up this machine'],
                  ['i', 'Create a new project'],
                  ['p', 'Adopt an existing project'],
                  ['n', 'Set up a remote node'],
                  ['e', 'Edit this project'],
                ]"
                :key="key"
                class="l"
              >
                <b class="t-key i2">{{ key }}</b
                ><span class="g2">{{ label }}</span>
              </div>
            </MmCard>
          </template>
        </div>
      </div>
    </div>

    <!-- Footer -->
    <div class="tui-footer">
      <template v-if="notice">
        <span>
          <MmBadge :tone="notice.tone">{{ notice.text }}</MmBadge>
          <span class="t-muted g3">any key dismisses</span>
        </span>
      </template>
      <template v-else>
        <MmHints :items="hints" class="tui-footer__local" />
        <MmHints
          v-if="!typing"
          :items="[
            ['?', 'help'],
            ['q', 'quit'],
          ]"
        />
      </template>
    </div>

    <!-- The action menu opens under the selected row. -->
    <div v-if="menu && !panel" class="tui-menu" :style="{ '--at': Math.min(scroll, 9) }">
      <MmCard :title="menu.label" focused class="tui-dialog">
        <template #meta>esc close</template>
        <div
          v-for="(item, index) in menuItems"
          :key="item.key"
          class="tui-menu__item"
          :class="{ 'is-selected': index === menu.selected, 'is-disabled': item.disabled }"
          @click="demo.interact(item.key)"
        >
          <b class="t-key">{{ item.key }}</b>
          <b class="t-strong">{{ item.label }}</b>
          <span class="t-muted">{{ item.disabled ? "not available here" : item.detail }}</span>
        </div>
      </MmCard>
    </div>

    <!-- Overlays -->
    <Transition name="tui-pop">
      <div v-if="confirm" class="tui-overlay">
        <MmCard :title="confirm.title" tone="warn" class="tui-dialog">
          <div v-for="line in confirm.lines" :key="line" class="t-soft tui-wrap">{{ line }}</div>
          <div class="l" />
          <div class="l">
            <MmHints
              :items="[
                ['enter/y', confirm.accept],
                ['esc/n', 'cancel'],
              ]"
            />
          </div>
        </MmCard>
      </div>
      <div v-else-if="help" class="tui-overlay">
        <MmCard title="Keys" focused class="tui-dialog tui-dialog--keys">
          <template #meta>esc close</template>
          <div class="l t-section">Navigation</div>
          <div v-for="[key, label] in helpRows" :key="key" class="l kv kv--16">
            <b class="t-key i2">{{ key }}</b
            ><span class="t-soft">{{ label }}</span>
          </div>
          <div class="l" />
          <div class="l t-section">General</div>
          <div class="l kv kv--16"><b class="t-key i2">?</b><span class="t-soft">toggle help</span></div>
          <div class="l kv kv--16"><b class="t-key i2">q</b><span class="t-soft">quit</span></div>
        </MmCard>
      </div>
      <div v-else-if="switching" class="tui-overlay">
        <MmCard title="Environment" focused class="tui-dialog tui-dialog--keys">
          <div class="l t-accent tui-busy">
            <MmSpinner /><b class="t-strong g1">Switching to {{ switching }}</b>
          </div>
          <div class="l t-muted">
            {{
              switchingTo?.host
                ? `Connecting to ${switchingTo.host}…`
                : switchingTo?.nodes
                  ? `Asking ${switchingTo.nodes.map((node) => node.name).join(", ")}…`
                  : "Reading its status…"
            }}
          </div>
          <div class="l" />
          <div class="l"><MmHints :items="[['esc', 'cancel']]" /></div>
        </MmCard>
      </div>
      <div v-else-if="prompt" class="tui-overlay">
        <MmCard title="Run a command" focused class="tui-dialog">
          <div class="l">
            <b class="t-strong">{{ prompt.label }}</b>
          </div>
          <div class="l t-muted">Runs the exact arguments: no shell, no TTY. For pipes use sh -c '…'.</div>
          <div class="l" />
          <div class="l">
            <b class="t-key">$</b><span class="g1">{{ prompt.input }}</span
            ><span class="tui-cursor" />
          </div>
          <div class="l" />
          <div class="l">
            <MmHints
              :items="[
                ['enter', 'run'],
                ['↑↓', 'history'],
                ['esc', 'cancel'],
              ]"
            />
          </div>
        </MmCard>
      </div>
      <div v-else-if="editor" class="tui-overlay">
        <MmCard v-if="editor.stage === 'form'" title="Add a teammate" focused class="tui-dialog">
          <template #meta><span class="hide-sm">nothing is saved until you review</span></template>
          <div class="t-muted tui-wrap">
            The users file in the repo decides who reaches each remote environment. A change takes effect once
            it is merged and deployed.
          </div>
          <div class="l" />
          <div
            v-for="(field, index) in editorFields"
            :key="field"
            class="l tui-field"
            :class="{ 'is-focused': index === editor.focus }"
          >
            <span class="tui-field__label">{{ fieldLabel(field) }}</span>
            <template v-if="field === 'name' || field === 'key'">
              <span>{{ editor[field] }}</span
              ><span v-if="index === editor.focus" class="tui-cursor" />
            </template>
            <template v-else>
              <span class="t-muted">{{ index === editor.focus ? "‹ " : "  " }}</span>
              <b :class="editor.levels[field] === LEVELS[0] ? 't-soft' : 't-key'">{{
                editor.levels[field]
              }}</b>
              <span v-if="index === editor.focus" class="t-muted"> ›</span>
            </template>
          </div>
          <div class="l" />
          <div class="t-soft tui-wrap">
            {{
              editor.focus === 0
                ? "Lowercase letters, digits, and dashes. For example alice."
                : editor.focus === 1
                  ? "They run `mm key create`, then `mm key show`, and send you the line starting ssh-ed25519."
                  : "none · view: status and logs · operate: + restart and jobs · deploy: + deploy · admin: + exec, forward, secrets"
            }}
          </div>
          <div class="l">
            <MmBadge v-if="editor.error" tone="bad">{{ editor.error }}</MmBadge>
          </div>
          <div class="l">
            <MmHints
              :items="[
                ['↑↓', 'field'],
                editor.focus > 1 ? ['← →', 'change'] : ['type', 'to edit'],
                ['enter', 'review'],
                ['esc', 'cancel'],
              ]"
            />
          </div>
        </MmCard>
        <MmCard
          v-else-if="editor.stage === 'review'"
          title="Review: Add a teammate"
          tone="ok"
          class="tui-dialog"
        >
          <template #meta>plan 5b1f0c2a9d3e</template>
          <div class="l">
            <span class="t-muted">Writes </span><b class="t-strong">users.yaml</b
            ><span class="t-muted">. Nothing else changes until you deploy.</span>
          </div>
          <div class="l" />
          <div class="l t-muted">--- .config/mm/users.yaml</div>
          <div class="l t-muted">+++ .config/mm/users.yaml</div>
          <div class="l t-info">@@ -9,0 +10,{{ editorDiff.length }} @@</div>
          <div v-for="line in editorDiff" :key="line" class="l t-accent">+{{ line }}</div>
          <div class="l" />
          <div class="l">
            <MmHints
              :items="[
                ['enter/y', 'apply'],
                ['b', 'back to the form'],
                ['esc/n', 'cancel'],
              ]"
            />
          </div>
        </MmCard>
        <MmCard v-else title="Add a teammate" focused class="tui-dialog">
          <div class="l">
            <MmBadge tone="ok">Added teammate '{{ editor.name }}' in .config/mm/users.yaml.</MmBadge>
          </div>
          <div class="l" />
          <div class="l t-section">What happens next</div>
          <div class="tui-step">
            <b class="t-key">1</b
            ><span>Commit .config/mm/users.yaml on a branch and open a pull request.</span>
          </div>
          <div class="tui-step">
            <b class="t-key">2</b
            ><span>After the merge, deploy each environment, or an admin runs `mm team sync` there.</span>
          </div>
          <div class="tui-step">
            <b class="t-key">3</b
            ><span
              >{{ editor.name }} opens an environment with `mm -e ENVIRONMENT` and checks the Access
              view.</span
            >
          </div>
          <div class="l" />
          <div class="l"><MmHints :items="[['enter', 'done']]" /></div>
        </MmCard>
      </div>
    </Transition>
  </div>
</template>

<style>
/*
 * Everything is sized in cells: 1ch wide, 1.5em tall. The font size is derived from the
 * container width so the terminal always shows the same number of columns.
 */
.tui {
  --tui-cols: 126;
  --tui-rows: 30;
  --row: 1.5em;

  position: relative;
  display: flex;
  flex-direction: column;
  height: calc(var(--tui-rows) * var(--row));
  font-family: var(--mm-font-mono);
  font-size: calc(100cqw / (var(--tui-cols) * 0.6));
  font-variant-ligatures: none;
  line-height: var(--row);
  color: var(--mm-ink);
  white-space: pre;
  cursor: default;
  user-select: none;
  outline: none;
}

.tui b {
  font-weight: 700;
}

.tui .t-strong {
  color: var(--mm-ink);
  font-weight: 700;
}
.tui .t-soft {
  color: var(--mm-ink-soft);
}
.tui .t-muted {
  color: var(--mm-muted);
}
.tui .t-accent {
  color: var(--mm-accent);
}
.tui .t-warn {
  color: var(--mm-warn);
}
.tui .t-info {
  color: var(--mm-info);
}
.tui .t-tone {
  color: var(--tone);
}
.tui .t-border {
  color: var(--mm-border);
}
.tui .t-key {
  color: var(--mm-accent);
  font-weight: 700;
}
.tui .t-section {
  color: var(--mm-muted);
  font-weight: 700;
  text-transform: uppercase;
}

.tui .l {
  height: var(--row);
  overflow: hidden;
  text-overflow: clip;
}

.tui .tui-wrap {
  white-space: pre-wrap;
}

.tui .mm-badge {
  gap: 1ch;
}

.tui .mm-tone {
  width: 1ch;
  height: 1ch;
  vertical-align: 0.02em;
  transform: scale(1.5);
}

/* Spacing in cells, instead of literal runs of spaces the template compiler would collapse. */
.tui .g1 {
  margin-left: 1ch;
}
.tui .g2 {
  margin-left: 2ch;
}
.tui .g3 {
  margin-left: 3ch;
}
.tui .g4 {
  margin-left: 4ch;
}
.tui .i2 {
  padding-left: 2ch;
}

.tui .mm-hints {
  gap: 0 3ch;
  flex-wrap: nowrap;
}

.tui .mm-card {
  --mm-card-pad: 1.5ch;
  --mm-card-radius: 0.5em;
  --mm-card-row: var(--row);
}

.tui .mm-card__title,
.tui .mm-card__meta {
  padding: 0 1ch;
}

.tui .mm-card__body {
  padding: 0 1.5ch calc(var(--row) / 2);
}

/* Header: one row of text, then a rule through the middle of the second row. */
.tui-header {
  position: relative;
  display: flex;
  flex: none;
  justify-content: space-between;
  gap: 2ch;
  height: calc(2 * var(--row));
  padding: 0 1ch;
}

.tui-header::after {
  position: absolute;
  right: 0;
  bottom: calc(var(--row) / 2);
  left: 0;
  content: "";
  border-top: 1px solid var(--mm-border);
}

.tui-identity,
.tui-status {
  display: flex;
  height: var(--row);
  min-width: 0;
}

/* A long status never runs under the identity: the identity gives way, the runtime first. */
.tui-identity {
  overflow: hidden;
}

.tui-status {
  gap: 3ch;
  flex: none;
}

.tui-brand {
  margin-right: 2ch;
  padding: 0 1ch;
  font-weight: 700;
  color: #000;
  background: var(--mm-accent);
}

.tui-tag {
  color: var(--mm-ink-soft);
}

/* The environment: tinted when local, solid when what you do happens on another machine. */
.tui-chip {
  margin: 0 2ch;
  padding: 0 1ch;
  font-weight: 700;
  color: var(--mm-accent);
  background: var(--mm-selection);
}

.tui-chip--remote {
  color: #000;
  background: var(--mm-info);
}

.tui-busy {
  display: inline-flex;
  align-items: center;
}

/* In a table cell the spinner sits on the row's own line instead of the text baseline. */
.tui .tr .tui-busy {
  display: flex;
  height: var(--row);
}

.tui-tag i {
  padding: 0 2ch;
  font-style: normal;
  color: var(--mm-border);
}

/* Body */
.tui-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.tui-sidebar {
  flex: none;
  width: 26ch;
  margin-top: calc(var(--row) * -0.5);
  padding-top: calc(var(--row) * 1.5);
  border-right: 1px solid var(--mm-border);
}

.tui-group {
  margin-bottom: var(--row);
}

.tui-group__title {
  padding-left: 2ch;
  font-weight: 700;
  color: var(--mm-muted);
  text-transform: uppercase;
}

.tui-nav {
  position: relative;
  display: flex;
  width: 100%;
  height: var(--row);
  padding: 0 1ch 0 2ch;
  font: inherit;
  color: var(--mm-ink-soft);
  text-align: left;
  cursor: pointer;
  background: none;
  border: 0;
  transition: background-color 0.18s var(--mm-ease);
}

.tui-nav:hover {
  background: rgb(255 255 255 / 4%);
}

.tui-nav__digit {
  width: 3ch;
  color: var(--mm-muted);
}

.tui-nav__title {
  flex: 1;
}

.tui-nav__meta {
  color: var(--tone);
}

.tui-nav.is-selected {
  font-weight: 700;
  color: var(--mm-ink);
  background: var(--mm-selection);
}

.tui-nav.is-selected .tui-nav__digit {
  color: var(--mm-accent);
}

.tui-nav.is-selected::before,
.tui .tr.is-selected::before,
.tui-finding.is-selected::before {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 0;
  width: 0.5ch;
  content: "";
  background: var(--mm-accent);
}

.tui-main {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
  padding: 0 1ch;
}

.tui-strip {
  display: none;
}

.tui-content {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
}

.tui-grow {
  flex: 1;
  min-height: 0;
}

/* Rows that do not fit end at the card's bottom edge, as a terminal cuts them off: whole rows only. */
.tui-grow > .mm-card__body {
  min-height: 0;
  overflow: hidden;
}

.tui-grow > .mm-card__body > .tt:only-child {
  max-height: round(down, 100%, var(--row));
  overflow: hidden;
}

.tui .tt > .tr {
  flex: none;
}

.tui-fixed {
  flex: none;
}

/* Overview */
.tui-stats {
  display: grid;
  flex: none;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 0 1ch;
}

.tui-split {
  display: grid;
  flex: 1;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0 1ch;
  min-height: 0;
}

.tui-panes {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 0 1ch;
}

.tui-panes .mm-card {
  --mm-card-pad: 1ch;
}

.tui-panes .mm-card__body {
  padding-inline: 0.5ch;
}

.tui .kv > span:first-child,
.tui .kv > b:first-child {
  display: inline-block;
  width: 13ch;
  color: var(--mm-muted);
}

.tui .kv > b:first-child {
  color: var(--mm-accent);
}

.tui .kv--10 > :first-child {
  width: 10ch;
}
.tui .kv--12 > :first-child {
  width: 12ch;
}
.tui .kv--16 > :first-child {
  width: 16ch;
}

.tui-empty {
  display: flex;
  flex-direction: column;
  gap: var(--row);
  align-items: center;
  justify-content: center;
  height: 100%;
  text-align: center;
}

/* Tables */
.tui .tt {
  display: flex;
  flex-direction: column;
}

.tui .tr {
  position: relative;
  display: grid;
  grid-template-columns: var(--tt);
  column-gap: 2ch;
  height: var(--row);
  padding-left: 1ch;
  overflow: hidden;
  cursor: pointer;
}

.tui .tr > * {
  min-width: 0;
  overflow: hidden;
}

.tui .tr.th {
  font-weight: 700;
  color: var(--mm-muted);
  cursor: default;
}

.tui .tr:not(.th, .is-selected):hover {
  background: rgb(255 255 255 / 3%);
}

.tui .tr.is-selected,
.tui-finding.is-selected {
  font-weight: 700;
  background: var(--mm-selection);
}

.tui .tr .mm-pips {
  font-size: 1.05em;
}

.tt--services {
  --tt: 12ch minmax(22ch, 1fr) 19ch 8ch;
}
.tt--environments {
  --tt: minmax(12ch, 1fr) 12ch 17ch;
}
.tt--team {
  --tt: minmax(16ch, 1fr) minmax(22ch, 1fr) 20ch;
}
.tt--ingress {
  --tt: 24% minmax(30ch, 1fr) 6ch 20ch;
}
.tt--environment {
  --tt: 12ch 18ch 12ch 17ch minmax(10ch, 1fr);
}
.tt--mounts {
  --tt: 18% minmax(30ch, 1fr) 11ch 13ch;
}
.tt--gates {
  --tt: minmax(16ch, 1fr) 12ch 8ch 8ch;
}
.tt--providers {
  --tt: 9ch 35% minmax(20ch, 1fr);
}
.tt--jobs {
  --tt: 0.8fr 1.2fr;
}
.tt--schedules {
  --tt: 5ch minmax(5ch, 1fr) 12ch;
}
.tt--runs {
  --tt: 11ch minmax(6ch, 1fr) 3ch;
}

/* Environment */
.tui-search {
  height: calc(2 * var(--row));
  background: linear-gradient(var(--mm-border), var(--mm-border)) 0 calc(var(--row) * 1.5) / 100% 1px
    no-repeat;
}

.tui [data-view="variables"] .mm-card__body {
  display: flex;
  flex-direction: column;
}

.tui [data-view="variables"] .tt {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.tui-detail {
  flex: none;
}

.tui-detail__title {
  display: flex;
  align-items: center;
  height: var(--row);
}

.tui-detail__title b {
  padding: 0 1ch;
}

.tui-detail__title::before,
.tui-detail__title::after {
  content: "";
  border-top: 1px solid var(--mm-border);
}

.tui-detail__title::before {
  width: 0;
}

.tui-detail__title::after {
  flex: 1;
}

/* Environments: the list beside the selected environment's details. */
.tui-pair {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 40ch;
  gap: 0 1ch;
}

/* Services: containers hang off their service, and the menu opens under the selection. */
.tui .i1 {
  padding-left: 1ch;
}

.tui-fold,
.tui-branch {
  display: inline-block;
  width: 2ch;
}

.tui-branch {
  margin-left: 2ch;
}

/* A node's row runs across every column after the state, as the TUI spans it. */
.tui .tr--node > .tui-node {
  grid-column: 2 / -1;
}

/* Rows above the first service: header 2, target card 4, card title 1, table header 1. */
.tui-menu {
  position: absolute;
  top: calc((var(--at) + 9) * var(--row));
  left: 44ch;
  z-index: 2;
  width: 64ch;
}

.tui-menu .tui-dialog {
  width: 100%;
}

.tui-menu__item {
  position: relative;
  display: grid;
  grid-template-columns: 2ch 16ch minmax(0, 1fr);
  column-gap: 1ch;
  height: var(--row);
  padding-left: 1.5ch;
  overflow: hidden;
  cursor: pointer;
}

.tui-menu__item.is-selected {
  background: var(--mm-selection);
}

.tui-menu__item.is-selected::before {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 0;
  width: 0.5ch;
  content: "";
  background: var(--mm-accent);
}

.tui-menu__item.is-disabled b {
  font-weight: 400;
  color: var(--mm-muted);
}

/* The review: a head that says what the plan does, ruled off from the plan itself. */
.tui-review .mm-card__body {
  overflow: hidden;
}

.tui-review__head {
  flex: none;
  padding-bottom: calc(var(--row) / 2 - 1px);
  margin-bottom: calc(var(--row) / 2);
  border-bottom: 1px solid var(--mm-border);
}

.tui-review__wrap,
.tui-review__line {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

/* An empty line of the plan keeps its row. */
.tui-review__line {
  min-height: var(--row);
}

.tui-review__plan {
  flex: 1;
  min-height: 0;
}

/* Rows that do not fit end at the card's edge, whole rows only, as a terminal cuts them off. */
.tui-review__rows {
  max-height: round(down, 100%, var(--row));
  overflow: hidden;
}

/* A spinner in a card's title row sits on that row, not below its baseline. */
.tui .mm-card__meta .tui-busy {
  height: var(--row);
  vertical-align: top;
}

/* Log and command output */
.tui-body--panel {
  padding: 0 1ch;
}

.tui-output .mm-card__body {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.tui-output__lines {
  display: flex;
  flex: 1;
  flex-direction: column;
  justify-content: flex-end;
  min-height: 0;
  overflow: hidden;
}

.tui-line {
  flex: none;
  padding-left: 2ch;
  color: var(--tone, var(--mm-ink));
}

.tui-line__time {
  margin-right: 1ch;
}

/* The rule between where a merged line came from and what it says. */
.tui-line__rule {
  margin: 0 1ch;
}

.tui-line.is-stderr {
  box-shadow: inset 0.5ch 0 0 var(--mm-warn);
}

/* Forms */
.tui-field {
  position: relative;
  padding-left: 2ch;
}

.tui-field__label {
  display: inline-block;
  width: 15ch;
  color: var(--mm-muted);
}

.tui-field.is-focused .tui-field__label {
  font-weight: 700;
  color: var(--mm-ink);
}

.tui-field.is-focused::before {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 0;
  width: 0.5ch;
  content: "";
  background: var(--mm-accent);
}

.tui-cursor {
  display: inline-block;
  width: 1ch;
  height: 1.1em;
  vertical-align: -0.2em;
  background: var(--mm-accent);
}

.tui-step {
  display: grid;
  grid-template-columns: 3ch minmax(0, 1fr);
  white-space: pre-wrap;
}

/* Diagnostics */
.tui-finding {
  position: relative;
  margin-bottom: var(--row);
  padding-left: 1ch;
  cursor: pointer;
}

/* Footer */
.tui-footer {
  display: flex;
  flex: none;
  justify-content: space-between;
  gap: 2ch;
  height: var(--row);
  padding: 0 1ch;
  overflow: hidden;
}

.tui-footer__local {
  min-width: 0;
  overflow: hidden;
}

/* A hint that does not fit wraps onto the hidden second row, so none is cut in the middle. */
.tui .tui-footer__local {
  flex-wrap: wrap;
  height: var(--row);
}

/* Overlays */
.tui-overlay {
  position: absolute;
  inset: 0;
  z-index: 2;
  display: grid;
  place-items: center;
  background: rgb(5 6 5 / 55%);
}

/* Solid from the border line down; the title and summary mask the line they sit on. */
.tui-dialog {
  --mm-card-surface: var(--tui-bg, var(--mm-bg));

  width: min(84ch, 92%);
}

.tui-dialog .mm-card__title,
.tui-dialog .mm-card__meta {
  background: var(--tui-bg, var(--mm-bg));
}

.tui-dialog--keys {
  width: 52ch;
}

.tui-pop-enter-active,
.tui-pop-leave-active {
  transition: opacity 0.16s var(--mm-ease);
}

.tui-pop-enter-active .tui-dialog,
.tui-pop-leave-active .tui-dialog {
  transition: transform 0.2s var(--mm-ease-out);
}

.tui-pop-enter-from,
.tui-pop-leave-to {
  opacity: 0;
}

.tui-pop-enter-from .tui-dialog {
  transform: translateY(0.6em) scale(0.985);
}

/* Narrow terminals: the sidebar collapses into a one-line strip, as it does below 100 columns. */
@media (max-width: 760px) {
  /* As tall as the cluster's Services view, so the first thing a phone shows has no empty rows. */
  .tui {
    --tui-cols: 60;
    --tui-rows: 20;
  }

  .tui-sidebar,
  .tui .hide-sm,
  .tui-tag,
  .tui-status__issues,
  .tui-header.is-busy .tui-status__count {
    display: none;
  }

  .tui-strip {
    position: relative;
    display: flex;
    flex: none;
    gap: 1ch;
    height: calc(2 * var(--row));
    padding: 0;
    font: inherit;
    color: inherit;
    text-align: left;
    background: none;
    border: 0;
  }

  .tui-strip::after {
    position: absolute;
    right: 0;
    bottom: calc(var(--row) / 2);
    left: 0;
    content: "";
    border-top: 1px solid var(--mm-border);
  }

  .tui-strip > * {
    height: var(--row);
  }

  .tui-strip__group {
    padding: 0 1ch;
    font-weight: 700;
    color: var(--mm-muted);
    text-transform: uppercase;
  }

  .tui-strip__tab {
    padding: 0 1ch;
    font-weight: 700;
    color: var(--mm-accent);
    background: var(--mm-selection);
  }

  .tui-status {
    gap: 2ch;
  }

  .tui-stats {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .tui-split,
  .tui-panes {
    grid-template-columns: minmax(0, 1fr);
  }

  .tt--services {
    --tt: 12ch minmax(14ch, 1fr) 14ch;
  }
  .tt--environments {
    --tt: minmax(12ch, 1fr) 13ch 17ch;
  }
  .tt--team {
    --tt: minmax(10ch, 1fr) minmax(18ch, 1fr);
  }
  .tui-pair {
    grid-template-columns: minmax(0, 1fr);
  }
  /* The tab strip adds two rows above the content. */
  .tui-menu {
    top: calc((var(--at) + 11) * var(--row));
    left: 2ch;
    width: calc(100% - 4ch);
  }
  .tui-menu__item {
    grid-template-columns: 2ch minmax(0, 1fr);
  }
  .tui-menu__item > :last-child {
    display: none;
  }
  .tui-chip {
    margin: 0 1ch;
  }
  .tt--ingress {
    --tt: 9ch minmax(24ch, 1fr) 5ch;
  }
  .tt--environment {
    --tt: 9ch minmax(18ch, 1fr) 12ch;
  }
  .tt--mounts {
    --tt: 9ch minmax(24ch, 1fr) 13ch;
  }
}
</style>
