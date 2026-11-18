<script setup lang="ts">
const { site } = useAppConfig();

const columns = [
  {
    title: "Start",
    links: [
      { label: "Install", to: "/docs/start/install" },
      { label: "Set up a cluster", to: "/docs/how-to/cluster" },
      { label: "Coming from Docker Swarm", to: "/docs/how-to/from-docker-swarm" },
      { label: "First local app", to: "/docs/tutorials/first-local-app" },
    ],
  },
  {
    title: "Learn",
    links: [
      { label: "How mm thinks", to: "/docs/concepts/mental-model" },
      { label: "Plans and apply", to: "/docs/concepts/plans-and-apply" },
      { label: "Add mm to a project", to: "/docs/onboarding" },
      { label: "Work with AI agents", to: "/docs/mcp" },
    ],
  },
  {
    title: "Reference",
    links: [
      { label: "CLI", to: "/docs/reference/cli" },
      { label: "Configuration", to: "/docs/reference/configuration" },
      { label: "Supported platforms", to: "/docs/reference/capability-matrix" },
      { label: "Known limitations", to: "/docs/reference/known-issues" },
    ],
  },
  {
    title: "Project",
    links: [
      { label: "GitHub", href: site.repo },
      { label: "Contributing", to: "/docs/project/contributing" },
      { label: "Security", to: "/docs/project/security" },
      { label: "License", to: "/docs/project/license" },
    ],
  },
];
</script>

<template>
  <footer class="footer">
    <div class="site-container">
      <div class="footer__top">
        <div class="footer__brand">
          <MmWordmark class="footer__wordmark" />
          <p>Turn a list of servers into one cluster that runs your Docker Compose project.</p>
          <a :href="site.vented" class="footer__vented" rel="noopener">
            <svg viewBox="0 0 684.96 536.87" width="22" height="17" aria-hidden="true" fill="currentColor">
              <polygon
                points="124.32 417.29 0 292.97 124.33 168.64 149.09 193.4 49.52 292.97 149.08 392.53 124.32 417.29"
              />
              <polygon
                points="535.87 193.41 560.64 168.64 684.96 292.97 560.64 417.29 535.89 392.53 635.44 292.97 535.87 193.41"
              />
              <polygon
                points="198.5 191.94 223.27 167.17 342.47 286.38 461.7 167.15 486.46 191.91 342.47 335.91 198.5 191.94"
              />
              <rect
                x="258.26"
                y="34.89"
                width="168.45"
                height="168.45"
                transform="translate(16.09 277.06) rotate(-45)"
              />
              <path
                fill-rule="evenodd"
                d="M173.62,267.46l-25.51,25.51,194.37,194.37,194.37-194.37-25.51-25.51-168.86,168.86-168.86-168.86ZM511.34,217.94l75.04,75.04-243.9,243.9-243.9-243.9,75.04-75.04,168.86,168.86,168.86-168.86Z"
              />
            </svg>
            <span>A <b>Vented</b> project</span>
          </a>
        </div>
        <nav v-for="column in columns" :key="column.title" class="footer__column" :aria-label="column.title">
          <h2>{{ column.title }}</h2>
          <ul>
            <li v-for="link in column.links" :key="link.label">
              <a v-if="'href' in link" :href="link.href" rel="noopener">{{ link.label }}</a>
              <NuxtLink v-else :to="link.to">{{ link.label }}</NuxtLink>
            </li>
          </ul>
        </nav>
      </div>
      <div class="footer__base">
        <span>MIT licensed.</span>
        <span class="mono">mm · {{ site.url.replace("https://", "") }}</span>
      </div>
    </div>
  </footer>
</template>

<style scoped>
.footer {
  position: relative;
  padding-block: clamp(3.5rem, 7vw, 6rem) 2rem;
  font-size: 0.9375rem;
  color: var(--mm-muted);
  border-top: 1px solid var(--mm-line);
}

.footer__top {
  display: grid;
  grid-template-columns: minmax(0, 1.5fr) repeat(4, minmax(0, 1fr));
  gap: 2.5rem 2rem;
}

.footer__brand {
  display: grid;
  gap: 1.1rem;
  align-content: start;
  justify-items: start;
  max-width: 19rem;
}

.footer__brand p {
  margin: 0;
  line-height: 1.5;
}

.footer__wordmark {
  font-size: 1.25rem;
}

.footer__vented {
  display: inline-flex;
  gap: 0.65rem;
  align-items: center;
  padding: 0.5rem 0.85rem 0.5rem 0.7rem;
  color: var(--mm-ink-soft);
  text-decoration: none;
  background: rgb(255 255 255 / 3%);
  border: 1px solid var(--mm-line);
  border-radius: 10px;
  transition:
    color 0.2s var(--mm-ease),
    border-color 0.2s var(--mm-ease);
}

.footer__vented:hover {
  color: var(--mm-ink);
  border-color: var(--mm-line-strong);
}

.footer__vented b {
  font-family: var(--mm-font-display);
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--mm-ink);
  text-transform: uppercase;
}

.footer__column h2 {
  margin: 0 0 1rem;
  font-family: var(--mm-font-mono);
  font-size: 0.75rem;
  font-weight: 600;
  letter-spacing: 0.12em;
  color: var(--mm-ink-soft);
  text-transform: uppercase;
}

.footer__column ul {
  display: grid;
  gap: 0.6rem;
  margin: 0;
  padding: 0;
  list-style: none;
}

.footer__column a {
  color: var(--mm-muted);
  text-decoration: none;
  transition: color 0.2s var(--mm-ease);
}

.footer__column a:hover {
  color: var(--mm-ink);
}

.footer__base {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem 2rem;
  justify-content: space-between;
  margin-top: clamp(2.5rem, 6vw, 4.5rem);
  padding-top: 1.5rem;
  font-size: 0.8125rem;
  border-top: 1px solid var(--mm-line);
}

@media (max-width: 900px) {
  .footer__top {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .footer__brand {
    grid-column: 1 / -1;
  }
}
</style>
