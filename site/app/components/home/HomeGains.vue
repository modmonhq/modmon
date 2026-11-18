<script setup lang="ts">
/*
 * What the nodes of a cluster get, in one row under the terminal. Each item opens its
 * detail in the cluster section further down (HomeCluster reads the same state).
 */
const topic = useClusterTopic();

const gains = [
  {
    topic: "network",
    title: "Private network",
    body: "WireGuard between every pair of nodes, with one UDP port open.",
  },
  {
    topic: "names",
    title: "Service names",
    body: "A container reaches postgres by name from any node.",
  },
  {
    topic: "deploys",
    title: "Rolling deploys",
    body: "One node at a time, and back to the last version on failure.",
  },
  {
    topic: "volumes",
    title: "Shared volumes",
    body: "One directory for the files replicas share. Databases stay on local disk.",
  },
  {
    topic: "logs",
    title: "One log",
    body: "Every container of a service on every node, in time order.",
  },
];
</script>

<template>
  <section class="gains" aria-label="What the nodes of a cluster get">
    <div class="site-container">
      <ul class="gains__row">
        <li v-for="(gain, index) in gains" :key="gain.topic" data-reveal :style="{ '--reveal-i': index }">
          <a href="#cluster" class="gain" @click="topic = gain.topic">
            <span class="gain__title"><MmTone tone="ok" />{{ gain.title }}</span>
            <span class="gain__body">{{ gain.body }}</span>
          </a>
        </li>
      </ul>
      <p class="gains__runs" data-reveal>
        <span>Nodes are Debian or Ubuntu hosts, and setup installs Docker or Podman on them.</span>
        <span>Your machine runs Linux or macOS.</span>
        <NuxtLink to="/docs/reference/capability-matrix" class="gains__link">
          Supported features
          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
            <path
              d="m6 3.5 4.5 4.5L6 12.5"
              fill="none"
              stroke="currentColor"
              stroke-width="1.6"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </NuxtLink>
      </p>
    </div>
  </section>
</template>

<style scoped>
.gains {
  padding-top: clamp(3rem, 6vw, 5rem);
}

.gains__row {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  margin: 0;
  padding: 0;
  list-style: none;
  border-top: 1px solid var(--mm-line-strong);
}

.gains__row li + li {
  border-left: 1px solid var(--mm-line);
}

.gain {
  display: grid;
  gap: 0.55rem;
  align-content: start;
  height: 100%;
  padding: 1.25rem clamp(0.9rem, 1.6vw, 1.4rem) 1.4rem;
  text-decoration: none;
  transition: background-color 0.25s var(--mm-ease);
}

.gain:hover {
  background: linear-gradient(180deg, rgb(var(--mm-accent-rgb) / 6%), transparent 80%);
}

.gain__title {
  display: inline-flex;
  gap: 0.6rem;
  align-items: center;
  font-family: var(--mm-font-display);
  font-size: 1.2rem;
  font-weight: 600;
  line-height: 1.15;
  color: var(--mm-ink);
}

.gain__title .mm-tone {
  font-size: 0.65rem;
}

.gain__body {
  font-size: 0.9063rem;
  line-height: 1.5;
  color: var(--mm-muted);
}

.gains__runs {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem 1.75rem;
  align-items: center;
  margin: 0;
  padding: 1rem clamp(0.9rem, 1.6vw, 1.4rem) 0;
  font-family: var(--mm-font-mono);
  font-size: 0.78rem;
  color: var(--mm-muted);
  border-top: 1px solid var(--mm-line);
}

.gains__link {
  display: inline-flex;
  gap: 0.4rem;
  align-items: center;
  margin-left: auto;
  color: var(--mm-ink-soft);
  text-decoration: none;
  transition: color 0.2s var(--mm-ease);
}

.gains__link:hover {
  color: var(--mm-accent);
}

@media (max-width: 1000px) {
  .gains__row {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .gains__row li + li {
    border-left: 0;
  }

  .gains__row li:nth-child(even) {
    border-left: 1px solid var(--mm-line);
  }

  .gains__row li:nth-child(n + 3) {
    border-top: 1px solid var(--mm-line);
  }

  .gains__row li:last-child {
    grid-column: 1 / -1;
  }
}

@media (max-width: 560px) {
  .gains__row {
    grid-template-columns: minmax(0, 1fr);
  }

  .gains__row li:nth-child(even) {
    border-left: 0;
  }

  .gains__row li + li {
    border-top: 1px solid var(--mm-line);
  }

  .gains__link {
    margin-left: 0;
  }
}
</style>
