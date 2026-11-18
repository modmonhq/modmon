<script setup lang="ts">
const small = [
  {
    tone: "info",
    title: "Jobs and schedules",
    body: "Declare migrations, cleanups, and recurring work next to your services. Run, retry, and cancel them from the CLI or the terminal interface.",
    to: "/docs/jobs-and-schedules",
    glyph: "mm job",
  },
  {
    tone: "ok",
    title: "A dashboard and a mailbox",
    body: "A local web dashboard shows status across projects, and Mailpit catches every email your services send in development.",
    to: "/docs/local-dashboard",
    glyph: "mm dev dashboard",
  },
  {
    tone: "warn",
    title: "Live code in running containers",
    body: "Development mounts map module source into running containers, with presets that keep dependencies and protected files out of the way.",
    to: "/docs/development-mounts",
    glyph: "mm dev mounts",
  },
] as const;

// One listener for the grid: each tile gets the pointer position in its own coordinates.
const bento = ref<HTMLElement>();
let frame = 0;

function onPointerMove(event: PointerEvent) {
  if (event.pointerType === "touch") return;
  cancelAnimationFrame(frame);
  frame = requestAnimationFrame(() => {
    for (const tile of bento.value?.querySelectorAll<HTMLElement>(".tile") ?? []) {
      const box = tile.getBoundingClientRect();
      tile.style.setProperty("--mx", `${event.clientX - box.left}px`);
      tile.style.setProperty("--my", `${event.clientY - box.top}px`);
    }
  });
}

onBeforeUnmount(() => cancelAnimationFrame(frame));
</script>

<template>
  <section id="development" class="section features">
    <div class="site-container">
      <header class="features__head">
        <h2 class="display display--lg" data-reveal>
          <span class="ink-metal">Develop on your laptop </span
          ><span class="ink-tone">with the config you deploy.</span>
        </h2>
        <p class="lede" data-reveal>
          Keep container commands, health checks, and volumes in Compose. mm adds trusted local HTTPS,
          secrets, source mounts, and jobs around them, and shows you each change before it happens.
        </p>
      </header>

      <div ref="bento" class="bento" @pointermove="onPointerMove">
        <!-- Plans -->
        <MmCard as="article" class="tile tile--plan" data-tone="warn" data-reveal>
          <template #title><MmTone tone="warn" />mm deploy plan</template>
          <div class="tile__body">
            <div class="tile__copy">
              <h3 class="display display--md">Nothing changes until you have read the plan.</h3>
              <p>
                Builds, deployments, and shared services each produce a plan and print its hash. Apply takes
                the hash of the plan you reviewed. If the config, a secret, or the target moved in between, mm
                stops and asks you to plan again.
              </p>
              <NuxtLink to="/docs/concepts/plans-and-apply" class="tile__link">How plans work</NuxtLink>
            </div>
            <div class="term" role="group" aria-label="Example: plan, then apply by hash">
              <div class="term__cmd"><span class="term__p">$</span><span>mm deploy plan</span></div>
              <div class="term__row">
                <i class="chg">~</i><span>api</span><i class="dim">image</i
                ><span><i class="dim">5d0c…</i> → 9b1e…</span>
              </div>
              <div class="term__row">
                <i class="chg">~</i><span>web</span><i class="dim">env</i><span>API_URL</span>
              </div>
              <div class="term__row">
                <i class="dim">=</i><i class="dim">worker</i><i class="dim">unchanged</i><span />
              </div>
              <div class="term__row term__row--hash">
                <span /><i class="dim">plan</i><b class="hash">7c1e9a4f</b><span />
              </div>
              <div class="term__gap" />
              <div class="term__cmd">
                <span class="term__p">$</span
                ><span>mm deploy apply --plan <b class="hash">7c1e9a4f</b> --yes</span>
              </div>
              <div class="term__row"><span /><MmBadge tone="ok" class="term__ok">applied</MmBadge></div>
            </div>
          </div>
        </MmCard>

        <!-- HTTPS -->
        <MmCard as="article" class="tile tile--https" data-tone="ok" data-reveal style="--reveal-i: 1">
          <template #title><MmTone tone="ok" />mm dev tls</template>
          <div class="tile__copy">
            <h3 class="display display--md">Real HTTPS on your laptop.</h3>
            <p>
              Wildcard <code>.test</code> DNS and a local certificate authority your machine trusts. Every
              routed service gets a hostname. No hosts-file entries, no browser warnings.
            </p>
          </div>
          <div class="urlbar">
            <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
              <path
                d="M4.5 7V5a3.5 3.5 0 0 1 7 0v2M3.5 7h9v6.5h-9z"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linejoin="round"
              />
            </svg>
            <span><i>https://</i>web.acme.mm.test</span>
          </div>
          <ul class="hosts">
            <li><span class="arrow">↗</span>api.acme.mm.test<i>8080</i></li>
            <li><span class="arrow">↗</span>web.acme.mm.test<i>3000</i></li>
            <li><MmBadge tone="ok">Trusted by your user certificate store</MmBadge></li>
          </ul>
          <NuxtLink to="/docs/local-https" class="tile__link">Local HTTPS</NuxtLink>
        </MmCard>

        <!-- Secrets -->
        <MmCard as="article" class="tile tile--third" data-tone="info" data-reveal>
          <template #title><MmTone tone="info" />mm secret</template>
          <div class="tile__copy">
            <h3 class="display display--md">Secrets never touch the repo.</h3>
            <p>
              Config holds references. Values live in a separate store: local, OpenBao, or Vault. Plans
              include the reference and never the value.
            </p>
          </div>
          <div class="vars">
            <div><span>DATABASE_URL</span><i>config</i><b>postgres://postgres:5432/acme</b></div>
            <div><span>SESSION_SECRET</span><i>secret</i><b class="dots">••••••</b></div>
            <div><span>LOG_LEVEL</span><i class="over">local override</i><b>debug</b></div>
          </div>
          <NuxtLink to="/docs/concepts/environments-and-secrets" class="tile__link"
            >Environments and secrets</NuxtLink
          >
        </MmCard>

        <!-- Config -->
        <MmCard as="article" class="tile tile--third" data-tone="ok" data-reveal style="--reveal-i: 1">
          <template #title><MmTone tone="ok" />.config/mm.yaml</template>
          <div class="tile__copy">
            <h3 class="display display--md">Your Compose files stay exactly as they are.</h3>
            <p>
              Image repositories, environments, secret references, and routes go in one file next to them.
              Commands, health checks, and volumes stay in Compose.
            </p>
          </div>
          <pre class="yaml" role="group" aria-label="Excerpt of .config/mm.yaml"><i>modules:</i>
  <i>backend:</i>
    <i>path:</i> backend
    <i>build:</i>
      <i>image:</i> ghcr.io/acme/api
    <i>services:</i>
      <i>api:</i> { <i>stack:</i> app, <i>role:</i> http }</pre>
          <NuxtLink to="/docs/concepts/configuration-ownership" class="tile__link">What goes where</NuxtLink>
        </MmCard>

        <!-- Team access -->
        <MmCard as="article" class="tile tile--third" data-tone="warn" data-reveal style="--reveal-i: 2">
          <template #title><MmTone tone="warn" />mm team</template>
          <div class="tile__copy">
            <h3 class="display display--md">A file in the repo decides who reaches each host.</h3>
            <p>
              The users file gives each person and each CI key a level per environment. A change takes effect
              when it is merged and deployed, and every node applies it.
            </p>
          </div>
          <div class="team" role="group" aria-label="Example: access levels in production">
            <div><span>alice</span><b>deploy</b></div>
            <div><span>bob</span><b>view</b></div>
            <div><span>ci</span><i>automation</i><b>deploy</b></div>
          </div>
          <NuxtLink to="/docs/remote-access" class="tile__link">Remote access</NuxtLink>
        </MmCard>

        <!-- Small trio -->
        <NuxtLink
          v-for="(item, index) in small"
          :key="item.title"
          :to="item.to"
          class="mini"
          :data-tone="item.tone"
          data-reveal
          :style="{ '--reveal-i': index }"
        >
          <span class="mini__glyph"><MmTone :tone="item.tone" />{{ item.glyph }}</span>
          <h3>{{ item.title }}</h3>
          <p>{{ item.body }}</p>
        </NuxtLink>
      </div>
    </div>
  </section>
</template>

<style scoped>
.features__head {
  display: grid;
  gap: 1.4rem;
  justify-items: start;
  max-width: 54rem;
  margin-bottom: clamp(2.5rem, 5vw, 4rem);
}

.bento {
  display: grid;
  grid-template-columns: repeat(12, minmax(0, 1fr));
  gap: 1.1rem;
}

/* Tiles are kit cards at marketing scale: the label rides the top border. */
.tile {
  /* Blocks inside a tile sit one pad from its edge, so tile radius = pad + block radius. */
  --tile-pad: 22px;
  --tile-inner: 8px;
  --mm-card-line: rgb(232 246 214 / 13%);
  --mm-card-radius: calc(var(--tile-pad) + var(--tile-inner));
  --mm-card-pad: var(--tile-pad);
  --mm-card-surface:
    radial-gradient(
      26rem circle at var(--mx, -60rem) var(--my, -60rem),
      rgb(var(--tone-rgb) / 7%),
      transparent 62%
    ),
    radial-gradient(ellipse 70% 55% at 100% 0%, rgb(var(--tone-rgb) / 7%), transparent 70%),
    linear-gradient(180deg, rgb(255 255 255 / 2.5%), transparent 45%);

  grid-column: span 12;
}

.tile:hover {
  --mm-card-line: rgb(var(--tone-rgb) / 45%);
}

.tile :deep(.mm-card__title) {
  display: inline-flex;
  gap: 0.6em;
  align-items: center;
  font-family: var(--mm-font-mono);
  font-size: 0.74rem;
  font-weight: 600;
  letter-spacing: 0.02em;
  color: var(--tone);
}

.tile :deep(.mm-card__body) {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
  padding-top: var(--mm-card-pad);
  padding-bottom: var(--mm-card-pad);
}

.tile--plan {
  grid-column: span 7;
}

.tile--https {
  grid-column: span 5;
}

.tile--third {
  grid-column: span 4;
}

.tile__body {
  display: grid;
  gap: 1.75rem;
}

.tile__copy {
  display: grid;
  gap: 0.8rem;
}

.tile__copy h3 {
  color: var(--mm-ink);
}

.tile__copy p {
  margin: 0;
  max-width: 34rem;
  font-size: 0.9688rem;
  line-height: 1.6;
  color: var(--mm-muted);
}

.tile__copy code {
  font-size: 0.9em;
  color: var(--mm-ink-soft);
}

.tile__link {
  display: inline-flex;
  gap: 0.4rem;
  align-items: center;
  align-self: flex-start;
  margin-top: auto;
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--mm-ink-soft);
  text-decoration: none;
  transition: color 0.2s var(--mm-ease);
}

.tile__link::after {
  content: "→";
  transition: transform 0.2s var(--mm-ease);
}

.tile__link:hover {
  color: var(--tone);
}

.tile__link:hover::after {
  transform: translateX(3px);
}

/* Plan terminal */
.term {
  padding: 1rem 1.15rem;
  overflow-x: auto;
  font-family: var(--mm-font-mono);
  font-size: 0.8125rem;
  line-height: 1.7;
  color: var(--mm-ink);
  background: var(--mm-bg-sunk);
  border: 1px solid var(--mm-line);
  border-radius: var(--tile-inner);
}

.term__p {
  color: var(--mm-accent);
}

.term__cmd {
  display: flex;
  gap: 0.75em;
}

.term__cmd b {
  font-weight: 600;
}

.term__row {
  display: grid;
  grid-template-columns: 1.6em 4.5em 4.5em 1fr;
  color: var(--mm-ink-soft);
}

.term__row--hash b {
  font-weight: 600;
}

.term__ok {
  grid-column: 2 / -1;
}

.term__gap {
  height: 0.7rem;
}

.term i {
  font-style: normal;
}

.term .chg,
.term .hash {
  color: var(--mm-warn);
}

.term .dim {
  color: var(--mm-muted);
}

/* HTTPS */
.urlbar {
  display: flex;
  gap: 0.6rem;
  align-items: center;
  padding: 0.6rem 0.9rem;
  font-family: var(--mm-font-mono);
  font-size: 0.875rem;
  color: var(--mm-accent);
  background: var(--mm-bg-sunk);
  border: 1px solid rgb(var(--mm-accent-rgb) / 28%);
  border-radius: 999px;
  box-shadow: 0 0 30px -12px rgb(var(--mm-accent-rgb) / 50%);
}

.urlbar span {
  overflow: hidden;
  color: var(--mm-ink);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.urlbar i {
  font-style: normal;
  color: var(--mm-muted);
}

.hosts {
  display: grid;
  gap: 0.4rem;
  margin: 0;
  padding: 0;
  font-family: var(--mm-font-mono);
  font-size: 0.8125rem;
  color: var(--mm-ink-soft);
  list-style: none;
}

.hosts li {
  display: flex;
  gap: 0.6rem;
  align-items: center;
}

.hosts .arrow {
  color: var(--mm-accent);
}

.hosts i {
  margin-left: auto;
  font-style: normal;
  color: var(--mm-muted);
}

.hosts li:last-child {
  margin-top: 0.4rem;
  padding-top: 0.75rem;
  border-top: 1px solid var(--mm-line);
}

/* Secrets */
.vars {
  display: grid;
  font-family: var(--mm-font-mono);
  font-size: 0.78rem;
}

.vars div {
  display: grid;
  grid-template-columns: minmax(0, 1.1fr) minmax(0, 0.9fr);
  gap: 0.1rem 0.75rem;
  padding: 0.55rem 0;
  border-top: 1px solid var(--mm-line);
}

.vars div:first-child {
  border-top: 0;
}

.vars span {
  color: var(--mm-ink);
}

.vars i {
  font-style: normal;
  color: var(--mm-muted);
  text-align: right;
}

.vars .over {
  color: var(--mm-accent);
}

.vars b {
  grid-column: 1 / -1;
  overflow: hidden;
  font-weight: 400;
  color: var(--mm-ink-soft);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.vars .dots {
  letter-spacing: 0.2em;
  color: var(--mm-muted);
}

/* Config */
.yaml {
  margin: 0;
  padding: 0.9rem 1rem;
  overflow-x: auto;
  font-family: var(--mm-font-mono);
  font-size: 0.78rem;
  line-height: 1.65;
  color: var(--mm-ink);
  background: var(--mm-bg-sunk);
  border: 1px solid var(--mm-line);
  border-radius: var(--tile-inner);
}

.yaml i {
  font-style: normal;
  color: var(--mm-accent);
}

/* Team access */
.team {
  display: grid;
  font-family: var(--mm-font-mono);
  font-size: 0.78rem;
}

.team div {
  display: flex;
  gap: 0.75rem;
  align-items: baseline;
  padding: 0.55rem 0;
  border-top: 1px solid var(--mm-line);
}

.team div:first-child {
  border-top: 0;
}

.team span {
  color: var(--mm-ink);
}

.team i {
  font-style: normal;
  color: var(--mm-muted);
}

.team b {
  margin-left: auto;
  font-weight: 500;
  color: var(--mm-warn);
}

/* Small trio */
.mini {
  display: grid;
  grid-column: span 4;
  gap: 0.6rem;
  align-content: start;
  padding: 1.4rem clamp(1.1rem, 2vw, 1.5rem) 1.5rem;
  text-decoration: none;
  border-top: 1px solid var(--mm-line-strong);
  transition:
    border-color 0.25s var(--mm-ease),
    background-color 0.25s var(--mm-ease);
}

.mini:hover {
  background: linear-gradient(180deg, rgb(var(--tone-rgb) / 5%), transparent 70%);
  border-color: var(--tone);
}

.mini__glyph {
  display: inline-flex;
  gap: 0.6rem;
  align-items: center;
  font-family: var(--mm-font-mono);
  font-size: 0.75rem;
  color: var(--mm-muted);
  white-space: nowrap;
}

.mini h3 {
  margin: 0.4rem 0 0;
  font-family: var(--mm-font-display);
  font-size: 1.3rem;
  font-weight: 600;
  line-height: 1.15;
  color: var(--mm-ink);
}

.mini p {
  margin: 0;
  font-size: 0.9375rem;
  line-height: 1.55;
  color: var(--mm-muted);
}

@media (min-width: 1000px) {
  .tile--plan .tile__body {
    grid-template-columns: minmax(0, 0.8fr) minmax(0, 1fr);
    flex: 1;
    align-items: center;
  }
}

@media (max-width: 999px) {
  .tile--plan,
  .tile--https {
    grid-column: span 12;
  }

  .tile--third,
  .mini {
    grid-column: span 6;
  }
}

@media (max-width: 680px) {
  .tile {
    --tile-pad: 18px;
  }

  .tile--third,
  .mini {
    grid-column: span 12;
  }
}
</style>
