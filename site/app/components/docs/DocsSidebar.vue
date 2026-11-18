<script setup lang="ts">
import type { DocsNavSection } from "~/composables/useDocs";

const props = defineProps<{ sections: DocsNavSection[]; current: string }>();

const uid = useId();
const nav = ref<HTMLElement>();

// Keep the current page in view inside the sidebar's own scroller.
function reveal(smooth: boolean) {
  const link = nav.value?.querySelector<HTMLElement>("[aria-current='page']");
  if (link) revealInScroller(link, smooth);
}

onMounted(() => reveal(false));
watch(
  () => props.current,
  () => nextTick(() => reveal(true)),
);
</script>

<template>
  <nav ref="nav" class="docs-nav" aria-label="Documentation pages">
    <section v-for="(section, index) in sections" :key="section.title" class="docs-nav__group">
      <h2 :id="`${uid}-${index}`" class="docs-nav__title">{{ section.title }}</h2>
      <ul class="docs-nav__list" :aria-labelledby="`${uid}-${index}`">
        <li v-for="item in section.items" :key="item.path">
          <NuxtLink
            :to="item.path"
            class="docs-nav__link"
            :class="{ 'is-current': item.path === current }"
            :aria-current="item.path === current ? 'page' : undefined"
          >
            {{ item.title }}
          </NuxtLink>
        </li>
      </ul>
    </section>
  </nav>
</template>

<style scoped>
.docs-nav {
  display: grid;
  gap: 1.75rem;
}

.docs-nav__title {
  margin: 0 0 0.5rem;
  padding-left: 0.875rem;
  font-family: var(--mm-font-mono);
  font-size: 0.6875rem;
  font-weight: 600;
  line-height: 1.6;
  letter-spacing: 0.14em;
  color: var(--mm-muted);
  text-transform: uppercase;
}

.docs-nav__list {
  display: grid;
  gap: 1px;
  margin: 0;
  padding: 0;
  list-style: none;
  box-shadow: inset 1px 0 0 var(--mm-line);
}

.docs-nav__link {
  position: relative;
  display: block;
  padding: 0.34rem 0.75rem 0.34rem 0.875rem;
  font-size: var(--mm-text-sm);
  line-height: 1.4;
  color: var(--mm-ink-soft);
  text-decoration: none;
  border-radius: 0 var(--mm-radius-sm) var(--mm-radius-sm) 0;
  transition:
    color 0.15s var(--mm-ease),
    background-color 0.15s var(--mm-ease);
}

/* The terminal interface's selection: a lime bar on the left edge and a tinted row. */
.docs-nav__link::before {
  position: absolute;
  inset: 0 auto 0 0;
  width: 1px;
  content: "";
  background: transparent;
  transition: background-color 0.15s var(--mm-ease);
}

.docs-nav__link:hover {
  color: var(--mm-ink);
  background: rgb(255 255 255 / 3%);
}

.docs-nav__link:hover::before {
  background: var(--mm-line-strong);
}

.docs-nav__link.is-current {
  font-weight: 600;
  color: var(--mm-ink);
  background: var(--mm-selection);
}

.docs-nav__link.is-current::before {
  width: 3px;
  background: var(--mm-accent);
}

.docs-nav__link:focus-visible {
  outline-offset: -2px;
}
</style>
