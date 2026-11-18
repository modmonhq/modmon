<script setup lang="ts">
definePageMeta({ layout: "docs" });

const route = useRoute();
const path = docsPath(route.path);

const { data: page } = await useDocsPage(path);
if (!page.value) {
  throw createError({ statusCode: 404, statusMessage: "Page not found", fatal: true });
}

const { pages } = await useDocsNav();
const index = computed(() => pages.value.findIndex((item) => item.path === path));
const previous = computed(() => (index.value > 0 ? pages.value[index.value - 1] : undefined));
const next = computed(() => (index.value >= 0 ? pages.value[index.value + 1] : undefined));

// Relative links in the Markdown are written for GitHub; ProseA resolves them against the
// file's path in the repository. The collection's `/docs` prefix matches the directory
// name, so the stem is that path without its extension.
provide(docsSourceKey, `${page.value.stem}.${page.value.extension}`);

// The template keeps one root element whatever happens to `page`: the page transition needs it.
const isOverview = path === "/docs";

useSeoMeta({
  title: isOverview ? "mm docs" : `${page.value.title} · mm docs`,
  ogTitle: isOverview ? "mm docs" : `${page.value.title} · mm docs`,
  description: page.value.description,
  ogDescription: page.value.description,
});
</script>

<template>
  <article class="docs-page">
    <template v-if="page">
      <header class="docs-page__head">
        <h1 class="docs-page__title">{{ page.title }}</h1>
        <p v-if="page.description" class="docs-page__lede">{{ page.description }}</p>
      </header>

      <DocsProse>
        <ContentRenderer :value="page" />
      </DocsProse>

      <DocsPager :previous="previous" :next="next" />
    </template>
  </article>
</template>

<style scoped>
.docs-page {
  /* About 72 characters of body text; tables and code may use the full column. */
  --docs-measure: 44rem;

  min-width: 0;
}

.docs-page__head {
  max-width: var(--docs-measure);
  margin-bottom: 2.5rem;
  padding-bottom: 2rem;
  border-bottom: 1px solid var(--mm-line);
}

.docs-page__title {
  margin: 0;
  font-family: var(--mm-font-display);
  font-size: clamp(2.125rem, 4.2vw, 3rem);
  font-weight: 600;
  line-height: 1.04;
  letter-spacing: -0.012em;
  color: var(--mm-ink);
  text-wrap: balance;
}

.docs-page__lede {
  margin: 1rem 0 0;
  font-size: clamp(1.0625rem, 1.5vw, 1.1875rem);
  line-height: 1.55;
  color: var(--mm-ink-soft);
  text-wrap: pretty;
}
</style>
