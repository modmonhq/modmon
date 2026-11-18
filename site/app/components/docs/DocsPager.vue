<script setup lang="ts">
import type { DocsNavItem } from "~/composables/useDocs";

/** Links to the pages before and after this one in the sidebar order. */
defineProps<{ previous?: DocsNavItem; next?: DocsNavItem }>();
</script>

<template>
  <nav v-if="previous || next" class="docs-pager" aria-label="Previous and next page">
    <NuxtLink v-if="previous" :to="previous.path" class="docs-pager__link" rel="prev">
      <MmCard as="div" class="docs-pager__card">
        <template #title>
          <span class="docs-pager__dir"><span aria-hidden="true">← </span>Previous</span>
        </template>
        <span class="docs-pager__title">{{ previous.title }}</span>
      </MmCard>
    </NuxtLink>
    <span v-else />
    <NuxtLink v-if="next" :to="next.path" class="docs-pager__link docs-pager__link--next" rel="next">
      <MmCard as="div" class="docs-pager__card">
        <template #meta>
          <span class="docs-pager__dir">Next<span aria-hidden="true"> →</span></span>
        </template>
        <span class="docs-pager__title">{{ next.title }}</span>
      </MmCard>
    </NuxtLink>
  </nav>
</template>

<style scoped>
.docs-pager {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1rem;
  margin-top: 4.5rem;
}

.docs-pager__link {
  display: block;
  color: inherit;
  text-decoration: none;
}

/* The card's frame turning lime is the focus indicator; a rectangular outline would not
   follow the frame, which starts half a row below the top of the link. */
.docs-pager__link:focus-visible {
  outline: none;
}

.docs-pager__card.mm-card {
  --mm-card-line: var(--mm-line-strong);
  --mm-card-pad: 1.1rem;

  height: 100%;
  font-family: var(--mm-font-mono);
  font-size: 0.75rem;
}

/* A definite row height: the card's rule is half of it, which a min-height cannot give. */
.docs-pager__card :deep(.mm-card__top) {
  height: var(--mm-card-row);
}

.docs-pager__card :deep(.mm-card__body) {
  display: grid;
  align-content: start;
  padding-block: 0.6rem 1rem;
}

.docs-pager__link--next .docs-pager__card :deep(.mm-card__body) {
  justify-items: end;
  text-align: right;
}

.docs-pager__dir {
  font-weight: 600;
  letter-spacing: 0.1em;
  color: var(--mm-muted);
  text-transform: uppercase;
  transition: color 0.2s var(--mm-ease);
}

.docs-pager__title {
  font-family: var(--mm-font-display);
  font-size: 1.1875rem;
  font-weight: 600;
  line-height: 1.25;
  color: var(--mm-ink);
  text-wrap: balance;
}

.docs-pager__link:hover .docs-pager__card.mm-card,
.docs-pager__link:focus-visible .docs-pager__card.mm-card {
  --mm-card-line: var(--mm-accent);
}

.docs-pager__link:focus-visible .docs-pager__card.mm-card {
  --mm-card-surface: rgb(var(--mm-accent-rgb) / 6%);
}

.docs-pager__link:hover .docs-pager__dir,
.docs-pager__link:focus-visible .docs-pager__dir {
  color: var(--mm-accent);
}

@media (max-width: 560px) {
  .docs-pager {
    grid-template-columns: minmax(0, 1fr);
  }

  .docs-pager > span {
    display: none;
  }
}
</style>
