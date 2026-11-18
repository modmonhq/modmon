<script setup lang="ts">
import type { DocsTocLink } from "~/composables/useDocs";

const route = useRoute();
const path = computed(() => docsPath(route.path));

const { sections, pages } = await useDocsNav();
const { data: page } = await useDocsPage(path);

// While the next page loads, keep showing the outline of the one still on screen.
const toc = shallowRef<DocsTocLink[]>([]);
watch(
  page,
  (next) => {
    if (next) toc.value = (next.body?.toc?.links ?? []) as DocsTocLink[];
  },
  { immediate: true },
);

const current = computed(() => pages.value.find((item) => item.path === path.value));
const active = useDocsScrollSpy(toc);

// The palette and the mobile sheet load on first use and stay mounted afterwards.
const search = reactive({ loaded: false, open: false });
const sheet = reactive({ loaded: false, open: false });

function openSearch() {
  sheet.open = false;
  search.loaded = true;
  search.open = true;
}

function openSheet() {
  sheet.loaded = true;
  sheet.open = true;
}

function onKeydown(event: KeyboardEvent) {
  if (event.defaultPrevented || event.isComposing) return;
  const palette = event.key.toLowerCase() === "k" && (event.metaKey || event.ctrlKey) && !event.altKey;
  if (palette) {
    event.preventDefault();
    if (search.open) search.open = false;
    else openSearch();
    return;
  }
  const target = event.target as HTMLElement | null;
  const typing = target?.closest("input, textarea, select, [contenteditable=''], [contenteditable='true']");
  if (event.key === "/" && !typing && !event.metaKey && !event.ctrlKey && !event.altKey && !search.open) {
    event.preventDefault();
    openSearch();
  }
}

onMounted(() => window.addEventListener("keydown", onKeydown));
onBeforeUnmount(() => window.removeEventListener("keydown", onKeydown));
watch(path, () => (sheet.open = false));

// Marks the document while the docs are on screen; see the unscoped style block below.
useHead({ htmlAttrs: { class: "is-docs" } });

// The index is fetched in the browser on first search, so nothing links to it for the
// prerenderer to find.
if (import.meta.server) prerenderRoutes("/docs-search.json");
</script>

<template>
  <div class="layout docs-layout">
    <a class="site-skip" href="#main">Skip to content</a>
    <SiteNav />
    <div class="docs-layout__backdrop" aria-hidden="true" />

    <div class="site-container docs-shell">
      <aside class="docs-shell__side" aria-label="Documentation navigation">
        <DocsSearchButton class="docs-shell__search" @click="openSearch" />
        <MmScrollArea class="docs-shell__scroll">
          <DocsSidebar :sections="sections" :current="path" />
        </MmScrollArea>
      </aside>

      <div class="docs-shell__center">
        <DocsLocalBar
          class="docs-shell__bar"
          :links="toc"
          :active="active"
          :group="current?.group"
          :title="current?.title"
          @menu="openSheet"
          @search="openSearch"
        />
        <main id="main" class="docs-shell__main">
          <slot />
        </main>
      </div>

      <div class="docs-shell__toc">
        <DocsToc v-if="toc.length" :links="toc" :active="active" />
      </div>
    </div>

    <SiteFooter />

    <LazyDocsSearch v-if="search.loaded" v-model:open="search.open" :pages="pages" />
    <LazyDocsNavSheet
      v-if="sheet.loaded"
      v-model:open="sheet.open"
      :sections="sections"
      :current="path"
      @search="openSearch"
    />
  </div>
</template>

<style scoped>
.docs-layout {
  --docs-side-w: 15rem;
  --docs-toc-w: 13rem;
  --docs-gap: clamp(2rem, 3.2vw, 3rem);
  --docs-top: calc(var(--site-nav-h) + 0.75rem);

  position: relative;
  /* Keeps the backdrop's negative z-index inside this layout. */
  isolation: isolate;
}

/* The hero's dot field, much quieter: enough to tie the pages together. */
.docs-layout__backdrop {
  position: absolute;
  inset: 0 0 auto;
  z-index: -1;
  height: 34rem;
  pointer-events: none;
  background:
    radial-gradient(ellipse 50% 60% at 50% 0%, rgb(var(--mm-accent-rgb) / 5%), transparent 70%),
    radial-gradient(rgb(232 246 214 / 13%) 1px, transparent 1.3px);
  background-size:
    100% 100%,
    26px 26px;
  mask-image: linear-gradient(180deg, #000 0%, transparent 100%);
}

.docs-shell {
  display: grid;
  grid-template-columns: var(--docs-side-w) minmax(0, 1fr) var(--docs-toc-w);
  gap: var(--docs-gap);
  align-items: start;
  padding-top: var(--docs-top);
}

.docs-shell__side,
.docs-shell__toc {
  position: sticky;
  top: var(--docs-top);
  max-height: calc(100vh - var(--docs-top));
}

.docs-shell__side {
  display: flex;
  flex-direction: column;
  height: calc(100vh - var(--docs-top));
  padding-top: 1.25rem;
}

.docs-shell__search {
  flex: none;
  margin-bottom: 1.25rem;
}

.docs-shell__scroll {
  flex: 1;
  margin-right: -0.75rem;
  mask-image: linear-gradient(180deg, transparent 0, #000 1rem, #000 calc(100% - 2.5rem), transparent 100%);
}

.docs-shell__scroll :deep(.docs-nav) {
  padding: 0.75rem 0.75rem 3rem 0;
}

.docs-shell__center {
  min-width: 0;
}

.docs-shell__bar {
  display: none;
}

.docs-shell__main {
  min-width: 0;
  padding-block: 1.5rem clamp(4rem, 8vw, 6rem);
}

.docs-shell__toc {
  padding-block: 1.75rem 2rem;
  overflow-y: auto;
  scrollbar-width: none;
}

@media (max-width: 1180px) {
  .docs-shell {
    grid-template-columns: var(--docs-side-w) minmax(0, 1fr);
  }

  .docs-shell__toc {
    display: none;
  }

  .docs-shell__bar {
    display: block;
  }
}

@media (max-width: 900px) {
  .docs-shell {
    grid-template-columns: minmax(0, 1fr);
  }

  .docs-shell__side {
    display: none;
  }

  .docs-shell__main {
    padding-top: 1.25rem;
  }
}
</style>

<style>
/*
 * Moving between pages should land at the top at once: with the site's smooth scrolling
 * the new page would be seen sliding past from wherever the last one was left.
 */
html.is-docs {
  scroll-behavior: auto;
}

/* The outline bar is stuck under the navigation at these widths; headings clear both. */
@media (max-width: 1180px) {
  html.is-docs {
    scroll-padding-top: calc(var(--site-nav-h) + 2.75rem + 1.25rem);
  }
}
</style>
