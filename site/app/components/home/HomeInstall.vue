<script setup lang="ts">
import { NuxtLink } from "#components";

const { site } = useAppConfig();

const platforms = [
  { value: "linux", label: "Linux" },
  { value: "macos", label: "macOS" },
] as const;

// Requirements mirror docs/start/install.md and docs/reference/capability-matrix.md.
const requirements = {
  linux: {
    arch: "x86-64 or ARM64",
    items: [
      "curl or wget, and tar",
      "Docker with Compose, or Podman Compose",
      "systemd, for local HTTPS and the dashboard",
    ],
  },
  macos: {
    arch: "Intel or Apple Silicon",
    items: ["Docker Desktop as the active Docker context", "Homebrew, for local HTTPS and the dashboard"],
  },
};

// What packaging/release/install.sh does.
const does = [
  "Downloads the latest release for your platform from GitHub over HTTPS.",
  "Checks the archive's SHA-256 against the release's SHA256SUMS before extracting it.",
  "Installs mm in ~/.local/bin, with its ingress helper on Linux. No sudo, and no toolchain to build.",
  "Replaces an existing mm atomically, and refuses to downgrade unless you pass --force.",
  "Later, mm update installs each new release the same way, checksum first.",
];
</script>

<template>
  <section id="install" class="section install" data-tone="ok">
    <div class="install__backdrop" aria-hidden="true" />
    <div class="site-container">
      <header class="install__head">
        <h2 class="display display--lg" data-reveal>
          <span class="ink-metal">Install mm with </span><span class="ink-tone">one command.</span>
        </h2>
        <p class="lede" data-reveal>
          The installer fetches a verified release and puts mm on your machine. Workstation setup is a
          separate step that shows its plan and asks before it changes anything.
        </p>
      </header>

      <div class="install__grid">
        <MmWindow class="install__window" data-reveal>
          <template #title>install</template>
          <div class="install__commands">
            <MmCommand :command="`curl --proto '=https' --tlsv1.2 -LsSf ${site.installUrl} | sh`" />
            <MmCommand command="mm dev setup" comment="local trust, ingress, dashboard, Mailpit" />
            <MmCommand command="mm" comment="open the guided interface" />
          </div>
          <div class="install__foot">
            <MmHints
              :items="[
                ['--version', 'pin a release'],
                ['--install-dir', 'choose where'],
                ['--with-daemon', 'also install mmd'],
              ]"
            />
          </div>
        </MmWindow>

        <div class="install__side" data-reveal style="--reveal-i: 1">
          <MmCard class="install__card">
            <template #title>you need</template>
            <MmTabs :tabs="platforms" label="Platform">
              <template v-for="platform in platforms" :key="platform.value" #[platform.value]>
                <p class="install__arch">{{ requirements[platform.value].arch }}</p>
                <ul class="install__list">
                  <li v-for="item in requirements[platform.value].items" :key="item">
                    <MmTone tone="ok" />{{ item }}
                  </li>
                </ul>
              </template>
            </MmTabs>
          </MmCard>

          <MmCard class="install__card">
            <template #title>the installer</template>
            <ul class="install__list install__list--plain">
              <li v-for="item in does" :key="item"><MmTone tone="idle" />{{ item }}</li>
            </ul>
          </MmCard>
        </div>
      </div>

      <div class="install__actions" data-reveal>
        <MmButton :as="NuxtLink" to="/docs/start/install" variant="primary" size="lg">
          Read the install guide
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
        <MmButton :as="NuxtLink" to="/docs/tutorials/first-local-app" size="lg"
          >Run your first local app</MmButton
        >
      </div>
    </div>
  </section>
</template>

<style scoped>
.install {
  overflow: clip;
}

.install__backdrop {
  position: absolute;
  inset: 0;
  z-index: -1;
  background:
    radial-gradient(ellipse 42% 36% at 50% 56%, rgb(var(--mm-accent-rgb) / 10%), transparent 72%),
    radial-gradient(rgb(232 246 214 / 9%) 1px, transparent 1.2px) 0 0 / 26px 26px;
  mask-image: linear-gradient(180deg, transparent 0%, #000 28%, #000 72%, transparent 100%);
}

.install__head {
  display: grid;
  gap: 1.4rem;
  justify-items: center;
  margin-bottom: clamp(2.5rem, 5vw, 3.75rem);
  text-align: center;
}

.install__grid {
  display: grid;
  gap: 1.25rem;
  max-width: 58rem;
  margin-inline: auto;
}

.install__window {
  box-shadow:
    0 0 0 1px rgb(232 246 214 / 15%),
    inset 0 1px 0 rgb(255 255 255 / 8%),
    0 50px 100px -50px rgb(0 0 0 / 90%),
    0 30px 80px -40px rgb(var(--mm-accent-rgb) / 22%);
}

.install__commands {
  display: grid;
  padding: 0.6rem 0.35rem;
}

.install__commands > * + * {
  border-top: 1px solid var(--mm-line);
}

.install__foot {
  padding: 0.75rem 1.3rem;
  font-size: 0.78rem;
  border-top: 1px solid var(--mm-line);
}

.install__side {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1.1rem;
}

/* Card radius = its padding + the radius of the tab strip that sits in its corner. */
.install__card {
  --install-tabs-radius: 8px;
  --mm-card-line: rgb(232 246 214 / 13%);
  --mm-card-pad: 16px;
  --mm-card-radius: calc(var(--mm-card-pad) + var(--install-tabs-radius));
}

.install__card :deep(.mm-tabs) {
  --mm-tabs-radius: var(--install-tabs-radius);
}

.install__card :deep(.mm-card__title) {
  font-family: var(--mm-font-mono);
  font-size: 0.74rem;
  font-weight: 600;
  letter-spacing: 0.12em;
  color: var(--mm-ink-soft);
  text-transform: uppercase;
}

.install__card :deep(.mm-card__body) {
  padding-top: var(--mm-card-pad);
  padding-bottom: 1.25rem;
}

.install__arch {
  margin: 0 0 0.75rem;
  font-family: var(--mm-font-mono);
  font-size: 0.78rem;
  color: var(--mm-muted);
}

.install__list {
  display: grid;
  gap: 0.6rem;
  margin: 0;
  padding: 0;
  font-size: 0.9375rem;
  line-height: 1.45;
  color: var(--mm-ink-soft);
  list-style: none;
}

.install__list li {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 0.75rem;
  align-items: start;
}

.install__list .mm-tone {
  margin-top: 0.38em;
  font-size: 0.7rem;
}

.install__list--plain {
  color: var(--mm-muted);
}

.install__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
  justify-content: center;
  margin-top: clamp(2.5rem, 5vw, 3.5rem);
}

@media (max-width: 760px) {
  .install__side {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
