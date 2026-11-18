<script setup lang="ts">
import { NuxtLink } from "#components";
import type { Tone } from "@mm/ui/types";

/*
 * Mirrors docs/mcp.md: the project's `agents:` block gives each environment a level
 * (view < operate < deploy) and optionally exec. A tool call outside the grant changes
 * nothing and returns the command for the user to run.
 */
type Level = "view" | "operate" | "deploy";

const levels = [
  { value: "view", label: "view" },
  { value: "operate", label: "operate" },
  { value: "deploy", label: "deploy" },
] as const;
const rank: Record<Level, number> = { view: 0, operate: 1, deploy: 2 };

const policy = reactive<Record<string, { level: Level; exec: boolean }>>({
  local: { level: "deploy", exec: true },
  staging: { level: "deploy", exec: false },
  production: { level: "view", exec: false },
});
const environments = Object.keys(policy);

/** The block as it would be written in .config/mm.yaml. */
const yaml = computed(() =>
  environments.map((name) => {
    const { level, exec } = policy[name]!;
    return { name, value: exec ? `{ level: ${level}, exec: true }` : level };
  }),
);

interface Call {
  tool: string;
  detail: string;
  environment: string;
  needs: Level | "exec";
  done: string;
  command: string;
}

const calls: Call[] = [
  {
    tool: "mm_read",
    detail: "nodes",
    environment: "production",
    needs: "view",
    done: "3 nodes ready · all on 3f2c9a1e5b7d",
    command: "mm --environment production target nodes",
  },
  {
    tool: "mm_job_apply",
    detail: "run migrate",
    environment: "staging",
    needs: "operate",
    done: "started · migrate attempt 1/1",
    command: "mm --environment staging job run migrate",
  },
  {
    tool: "mm_deploy_apply",
    detail: "plan 7c1e9a4f",
    environment: "staging",
    needs: "deploy",
    done: "applied · api 3/3 healthy",
    command: "mm --environment staging deploy apply --plan 7c1e9a4f --yes",
  },
  {
    tool: "mm_exec",
    detail: "api -- rails db:migrate",
    environment: "production",
    needs: "exec",
    done: "exit 0",
    command: "mm --environment production service exec api -- rails db:migrate",
  },
];

function outcome(call: Call): { allowed: boolean; tone: Tone; text: string } {
  const grant = policy[call.environment]!;
  const allowed = call.needs === "exec" ? grant.exec : rank[grant.level] >= rank[call.needs];
  return allowed
    ? { allowed, tone: "ok", text: call.done }
    : { allowed, tone: "info", text: `handoff · needs ${call.needs}, you run:` };
}

const points = [
  {
    title: "An agent reads the cluster the way you do",
    body: "Nodes, the mesh, name resolution, deployments, and logs come back as the same structured facts the CLI prints, so an agent works from what the hosts report.",
  },
  {
    title: "One reviewed block sets the limits",
    body: "The agents block in .config/mm.yaml gives each environment a level and, separately, exec. It takes effect only after you trust it on your machine, and again after every edit.",
  },
  {
    title: "Past the limit, the agent hands you the command",
    body: "A call outside the grant changes nothing. It returns the exact mm command, and the agent asks you to run it in your own terminal.",
  },
  {
    title: "Every node enforces the limits itself",
    body: "An agent connects with its own key and gets only what the deployed commit allows. mm refuses to run inside a coding tool's shell, and your own key stays behind a passphrase, so an agent cannot pick it up with ssh.",
  },
];
</script>

<template>
  <section id="agents" class="section agents" data-tone="info">
    <div class="agents__backdrop" aria-hidden="true" />
    <div class="site-container agents__grid">
      <div class="agents__copy">
        <h2 class="display display--lg" data-reveal>
          <span class="ink-metal">Decide what a coding agent may do</span><br />
          <span class="ink-tone">in each environment.</span>
        </h2>
        <p class="lede" data-reveal>
          <code class="agents__cmd">mm agent setup</code> connects Claude Code, OpenCode, Codex, or Cursor to
          mm's MCP server. An agent can work freely in local, run jobs and deploy in staging, and only read
          nodes, status, and logs on the production cluster.
        </p>
        <ul class="agents__points">
          <li
            v-for="(point, index) in points"
            :key="point.title"
            data-reveal
            :style="{ '--reveal-i': index }"
          >
            <MmTone tone="info" />
            <div>
              <h3>{{ point.title }}</h3>
              <p>{{ point.body }}</p>
            </div>
          </li>
        </ul>
        <div class="agents__actions" data-reveal>
          <MmButton :as="NuxtLink" to="/docs/mcp" size="lg">
            Let an agent work on your project
            <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
              <path
                d="m6 3.5 4.5 4.5L6 12.5"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </MmButton>
        </div>
      </div>

      <div class="agents__demo" data-reveal>
        <MmWindow class="agents__window">
          <template #title>.config/mm.yaml</template>
          <template #aside><MmBadge tone="ok" variant="pill">trusted</MmBadge></template>
          <div class="agents__panel">
            <MmCard class="agents__card">
              <template #title>agents</template>
              <template #meta>change a level</template>
              <ul class="grants">
                <li v-for="name in environments" :key="name" class="grant">
                  <code class="grant__name">{{ name }}</code>
                  <MmSegmented v-model="policy[name]!.level" :options="levels" :label="`Level in ${name}`" />
                  <label class="grant__exec" :for="`exec-${name}`">
                    exec
                    <MmSwitch :id="`exec-${name}`" v-model="policy[name]!.exec" />
                  </label>
                </li>
              </ul>
              <div class="block" role="group" aria-label="The block as written in the config file">
                <div><i>agents:</i></div>
                <div class="block__line"><i>"*":</i> view</div>
                <div v-for="line in yaml" :key="line.name" class="block__line">
                  <i>{{ line.name }}:</i> {{ line.value }}
                </div>
              </div>
            </MmCard>

            <MmCard class="agents__card" aria-live="polite">
              <template #title>agent session</template>
              <ul class="calls">
                <li v-for="call in calls" :key="call.tool" class="call">
                  <div class="call__request">
                    <span class="call__who">agent</span>
                    <span>
                      <b>{{ call.tool }}</b> {{ call.detail }}
                      <i class="call__env">{{ call.environment }}</i>
                    </span>
                  </div>
                  <div class="call__result" :data-tone="outcome(call).tone">
                    <span class="call__who">mm</span>
                    <span class="call__body">
                      <MmBadge :tone="outcome(call).tone">{{ outcome(call).text }}</MmBadge>
                      <code v-if="!outcome(call).allowed" class="call__command">{{ call.command }}</code>
                    </span>
                  </div>
                </li>
              </ul>
            </MmCard>
          </div>
        </MmWindow>
      </div>
    </div>
  </section>
</template>

<style scoped>
.agents {
  overflow: clip;
}

.agents__backdrop {
  position: absolute;
  inset: 0;
  z-index: -1;
  background:
    radial-gradient(ellipse 46% 50% at 78% 46%, rgb(var(--mm-info-rgb) / 9%), transparent 70%),
    radial-gradient(ellipse 30% 30% at 10% 10%, rgb(var(--mm-accent-rgb) / 4%), transparent 70%);
}

.agents__grid {
  display: grid;
  grid-template-columns: minmax(0, 0.92fr) minmax(0, 1.08fr);
  gap: clamp(2.5rem, 6vw, 6rem);
  align-items: center;
}

.agents__copy {
  display: grid;
  gap: 1.5rem;
  justify-items: start;
}

.agents__copy h2 {
  font-size: clamp(2rem, 3.5vw, 2.9rem);
  line-height: 1.04;
}

.agents__cmd {
  padding: 0.08em 0.45em;
  font-size: 0.9em;
  color: var(--mm-info);
  white-space: nowrap;
  background: rgb(var(--mm-info-rgb) / 9%);
  border: 1px solid rgb(var(--mm-info-rgb) / 22%);
  border-radius: 5px;
}

.agents__points {
  display: grid;
  gap: 1.15rem;
  margin: 0.5rem 0 0;
  padding: 0;
  list-style: none;
}

.agents__points li {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 0.9rem;
  align-items: start;
}

.agents__points .mm-tone {
  margin-top: 0.38rem;
  font-size: 0.8rem;
}

.agents__points h3 {
  margin: 0 0 0.2rem;
  font-size: 1rem;
  font-weight: 600;
  color: var(--mm-ink);
}

.agents__points p {
  margin: 0;
  max-width: 30rem;
  font-size: 0.9375rem;
  line-height: 1.55;
  color: var(--mm-muted);
}

.agents__actions {
  margin-top: 0.5rem;
}

/* Demo */
.agents__demo {
  position: relative;
  min-width: 0;
}

.agents__demo::before {
  position: absolute;
  inset: 8% -4% -8%;
  z-index: -1;
  content: "";
  background: radial-gradient(ellipse 60% 50% at 50% 60%, rgb(var(--mm-info-rgb) / 20%), transparent 70%);
  filter: blur(50px);
}

/* Window radius 20, panel inset 14: the cards inside take the 6 that is left. */
.agents__window {
  --mm-window-radius: 20px;

  box-shadow:
    0 0 0 1px rgb(214 236 255 / 14%),
    inset 0 1px 0 rgb(255 255 255 / 8%),
    0 50px 100px -50px rgb(0 0 0 / 90%);
}

.agents__panel {
  display: grid;
  gap: 0.9rem;
  padding: 14px;
  font-family: var(--mm-font-mono);
  font-size: 0.8125rem;
}

.agents__card {
  --mm-card-line: rgb(232 246 214 / 14%);
  --mm-card-radius: 6px;
  --mm-card-pad: 0.9rem;
}

.agents__card :deep(.mm-card__title) {
  font-size: 0.72rem;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: var(--mm-ink-soft);
  text-transform: uppercase;
}

.agents__card :deep(.mm-card__meta) {
  font-size: 0.72rem;
}

/* Grants */
.grants {
  display: grid;
  margin: 0;
  padding: 0.15rem 0 0;
  list-style: none;
}

.grant {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem 0.9rem;
  align-items: center;
  padding: 0.5rem 0;
  border-top: 1px solid var(--mm-line);
}

.grant:first-child {
  border-top: 0;
}

.grant__name {
  flex: 1;
  min-width: 5.5rem;
  color: var(--mm-ink);
}

.grant__exec {
  display: inline-flex;
  gap: 0.55rem;
  align-items: center;
  color: var(--mm-muted);
  cursor: pointer;
}

.block {
  margin-top: 0.6rem;
  padding-top: 0.75rem;
  font-size: 0.78rem;
  line-height: 1.6;
  color: var(--mm-ink-soft);
  border-top: 1px solid var(--mm-line);
}

.block__line {
  padding-left: 2ch;
}

.block i {
  font-style: normal;
  color: var(--mm-info);
}

/* Session */
.calls {
  display: grid;
  gap: 0.75rem;
  margin: 0;
  padding: 0.25rem 0 0;
  list-style: none;
}

.call {
  display: grid;
  gap: 0.2rem;
}

.call__request,
.call__result {
  display: grid;
  grid-template-columns: 3.4rem minmax(0, 1fr);
  gap: 0.5rem;
  line-height: 1.5;
  color: var(--mm-ink-soft);
}

.call__who {
  color: var(--mm-faint);
}

.call__request .call__who {
  color: var(--mm-info);
}

.call__request b {
  font-weight: 600;
  color: var(--mm-ink);
}

.call__env {
  margin-left: 0.5em;
  font-style: normal;
  color: var(--mm-muted);
}

.call__env::before {
  content: "· ";
}

.call__body {
  display: grid;
  gap: 0.15rem;
  justify-items: start;
  min-width: 0;
}

.call__body .mm-badge {
  white-space: normal;
}

.call__command {
  max-width: 100%;
  padding-left: 1.5em;
  overflow-wrap: anywhere;
  color: var(--mm-ink);
}

@media (max-width: 960px) {
  .agents__grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
