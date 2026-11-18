<script setup lang="ts">
import type { DocsTocLink } from "~/composables/useDocs";

/** The outline of the current page. `active` is the heading the reader is under. */
const props = withDefaults(defineProps<{ links: DocsTocLink[]; active?: string; heading?: boolean }>(), {
  active: undefined,
  heading: true,
});
defineEmits<{ pick: [] }>();

const nav = ref<HTMLElement>();

// Long outlines scroll on their own; follow the reader down them.
watch(
  () => props.active,
  () => {
    const link = nav.value?.querySelector<HTMLElement>("[aria-current='location']");
    if (link) revealInScroller(link, false, 32);
  },
  { flush: "post" },
);
</script>

<template>
  <nav ref="nav" class="docs-toc" aria-label="On this page">
    <h2 v-if="heading" class="docs-toc__title">On this page</h2>
    <ul class="docs-toc__list">
      <template v-for="link in links" :key="link.id">
        <li>
          <a
            :href="`#${link.id}`"
            class="docs-toc__link"
            :class="{ 'is-active': link.id === active }"
            :aria-current="link.id === active ? 'location' : undefined"
            @click="$emit('pick')"
          >
            {{ link.text }}
          </a>
        </li>
        <li v-for="child in link.children" :key="child.id">
          <a
            :href="`#${child.id}`"
            class="docs-toc__link docs-toc__link--nested"
            :class="{ 'is-active': child.id === active }"
            :aria-current="child.id === active ? 'location' : undefined"
            @click="$emit('pick')"
          >
            {{ child.text }}
          </a>
        </li>
      </template>
    </ul>
  </nav>
</template>

<style scoped>
.docs-toc__title {
  margin: 0 0 0.75rem;
  font-family: var(--mm-font-mono);
  font-size: 0.6875rem;
  font-weight: 600;
  line-height: 1.6;
  letter-spacing: 0.14em;
  color: var(--mm-muted);
  text-transform: uppercase;
}

.docs-toc__list {
  margin: 0;
  padding: 0;
  list-style: none;
  border-left: 1px solid var(--mm-line);
}

.docs-toc__link {
  position: relative;
  display: block;
  margin-left: -1px;
  padding: 0.26rem 0 0.26rem 0.875rem;
  font-size: 0.8125rem;
  line-height: 1.4;
  color: var(--mm-muted);
  text-decoration: none;
  text-wrap: pretty;
  border-left: 1px solid transparent;
  transition:
    color 0.15s var(--mm-ease),
    border-color 0.15s var(--mm-ease);
}

.docs-toc__link--nested {
  padding-left: 1.625rem;
}

.docs-toc__link:hover {
  color: var(--mm-ink);
}

.docs-toc__link.is-active {
  color: var(--mm-ink);
  border-left-color: var(--mm-accent);
}

.docs-toc__link:focus-visible {
  outline-offset: 0;
}
</style>
