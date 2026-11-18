<script setup lang="ts">
import type { NuxtError } from "#app";

const props = defineProps<{ error: NuxtError }>();

const route = useRoute();
const missing = computed(() => props.error.statusCode === 404);
const heading = computed(() => (missing.value ? "Page not found" : "Something went wrong"));

const links = [
  { label: "Documentation", to: "/docs" },
  { label: "Install mm", to: "/docs/start/install" },
  { label: "CLI command reference", to: "/docs/reference/cli" },
  { label: "Troubleshooting", to: "/docs/help/troubleshooting" },
];

// The error has to be cleared for the next page to render.
function leave(to: string) {
  return clearError({ redirect: to });
}

useSeoMeta({ title: () => `${heading.value} · mm`, robots: "noindex" });
</script>

<template>
  <NuxtLayout name="default">
    <section class="site-error site-container">
      <h1 class="display display--lg">{{ heading }}</h1>
      <p class="lede">
        <template v-if="missing">
          Nothing is served at this address. The documentation now lives under
          <code>/docs</code>, so an older link may point at a page that has moved.
        </template>
        <template v-else>The page could not be shown. Reloading usually fixes it.</template>
      </p>

      <MmCard as="div" class="site-error__card" title="Request" tone="bad">
        <dl class="site-error__rows">
          <div>
            <dt>Path</dt>
            <dd>{{ route.path }}</dd>
          </div>
          <div>
            <dt>Status</dt>
            <dd>
              <MmBadge tone="bad">{{ error.statusCode }} {{ missing ? "not found" : "error" }}</MmBadge>
            </dd>
          </div>
        </dl>
      </MmCard>

      <div class="site-error__actions">
        <MmButton as="a" href="/docs" variant="primary" size="lg" @click.prevent="leave('/docs')">
          Open the docs
        </MmButton>
        <MmButton as="a" href="/" size="lg" @click.prevent="leave('/')">Back to the home page</MmButton>
      </div>

      <nav class="site-error__links" aria-label="Suggested pages">
        <a v-for="link in links" :key="link.to" :href="link.to" @click.prevent="leave(link.to)">
          <MmTone tone="info" />
          {{ link.label }}
        </a>
      </nav>
    </section>
  </NuxtLayout>
</template>

<style scoped>
.site-error {
  display: grid;
  gap: 1.5rem;
  justify-items: start;
  min-height: 78vh;
  align-content: center;
  padding-block: calc(var(--site-nav-h) + 4rem) 6rem;
}

.site-error .lede code {
  padding: 0.1em 0.38em;
  font-size: 0.875em;
  color: var(--mm-ink);
  background: rgb(232 246 214 / 6%);
  border: 1px solid var(--mm-line);
  border-radius: 4px;
}

.site-error__card {
  width: min(100%, 34rem);
  margin-top: 0.5rem;
  font-family: var(--mm-font-mono);
  font-size: var(--mm-text-sm);
}

/* A definite row height: the card's rule is half of it, which a min-height cannot give. */
.site-error__card :deep(.mm-card__top) {
  height: var(--mm-card-row);
}

.site-error__rows {
  display: grid;
  gap: 0.3rem;
  margin: 0;
  padding-top: 0.35rem;
}

.site-error__rows > div {
  display: grid;
  grid-template-columns: 5.5rem minmax(0, 1fr);
  gap: 1rem;
}

.site-error__rows dt {
  color: var(--mm-muted);
}

.site-error__rows dd {
  margin: 0;
  color: var(--mm-ink);
  overflow-wrap: anywhere;
}

.site-error__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
  margin-top: 0.5rem;
}

.site-error__links {
  display: flex;
  flex-wrap: wrap;
  gap: 0.6rem 1.75rem;
  margin-top: 1rem;
  font-size: var(--mm-text-sm);
}

.site-error__links a {
  display: inline-flex;
  gap: 0.55em;
  align-items: center;
  color: var(--mm-ink-soft);
  text-decoration: none;
  transition: color 0.2s var(--mm-ease);
}

.site-error__links a:hover {
  color: var(--mm-ink);
}
</style>
