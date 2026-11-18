<script setup lang="ts">
import { CollapsibleContent, CollapsibleRoot, CollapsibleTrigger } from "reka-ui";
import type { DocsTocLink } from "~/composables/useDocs";

/**
 * The bar that stands in for the sidebar and the outline when there is no room for them:
 * a menu button, the page outline as a collapsible, and search.
 */
const props = defineProps<{ links: DocsTocLink[]; active?: string; group?: string; title?: string }>();
defineEmits<{ menu: []; search: [] }>();

const open = ref(false);
const route = useRoute();

const section = computed(() => {
  for (const link of props.links) {
    if (link.id === props.active) return link.text;
    const child = link.children?.find((item) => item.id === props.active);
    if (child) return child.text;
  }
  return undefined;
});

watch(
  () => route.path,
  () => (open.value = false),
);
</script>

<template>
  <CollapsibleRoot v-model:open="open" class="docs-bar" :disabled="!links.length" @keydown.esc="open = false">
    <div class="docs-bar__row">
      <button type="button" class="docs-bar__menu" @click="$emit('menu')">
        <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
          <path
            d="M2.5 4.5h11M2.5 8h7M2.5 11.5h11"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
          />
        </svg>
        <span>Menu</span>
      </button>
      <CollapsibleTrigger v-if="links.length" class="docs-bar__outline">
        <span class="docs-bar__label">On this page</span>
        <span class="docs-bar__section">{{ section ?? title }}</span>
        <svg class="docs-bar__chevron" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
          <path
            d="m4 6 4 4 4-4"
            fill="none"
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </CollapsibleTrigger>
      <p v-else class="docs-bar__outline docs-bar__outline--static">
        <span class="docs-bar__label">{{ group }}</span>
        <span class="docs-bar__section">{{ title }}</span>
      </p>
      <DocsSearchButton class="docs-bar__search" compact @click="$emit('search')" />
    </div>
    <CollapsibleContent class="docs-bar__panel">
      <DocsToc class="docs-bar__toc" :links="links" :active="active" :heading="false" @pick="open = false" />
    </CollapsibleContent>
  </CollapsibleRoot>
</template>

<style scoped>
.docs-bar {
  /* Controls sit --docs-bar-gap inside the border, so their radius is the bar's less that. */
  --docs-bar-radius: 12px;
  --docs-bar-gap: 5px;
  --docs-bar-h: 2.75rem;
  --docs-bar-inner-radius: calc(var(--docs-bar-radius) - 1px - var(--docs-bar-gap));

  position: sticky;
  top: calc(var(--site-nav-h) + 0.1rem);
  z-index: 40;
  margin-inline: calc(var(--site-gutter) * -1 + clamp(0.6rem, 2.4vw, 1.5rem));
  background: rgb(10 13 11 / 90%);
  border: 1px solid var(--mm-line-strong);
  border-radius: var(--docs-bar-radius);
  box-shadow:
    inset 0 1px 0 rgb(255 255 255 / 4%),
    0 14px 40px -18px rgb(0 0 0 / 80%);
  backdrop-filter: blur(18px) saturate(150%);
}

.docs-bar__row {
  display: flex;
  gap: 0.25rem;
  align-items: center;
  height: var(--docs-bar-h);
  padding: 0 var(--docs-bar-gap);
}

.docs-bar__menu,
.docs-bar__outline {
  display: flex;
  gap: 0.55rem;
  align-items: center;
  height: calc(var(--docs-bar-h) - 2 * var(--docs-bar-gap));
  padding: 0 0.65rem;
  font-family: var(--mm-font-sans);
  font-size: var(--mm-text-sm);
  color: var(--mm-ink-soft);
  cursor: pointer;
  background: none;
  border: 0;
  border-radius: var(--docs-bar-inner-radius);
  transition:
    color 0.2s var(--mm-ease),
    background-color 0.2s var(--mm-ease);
}

.docs-bar__menu {
  flex: none;
  font-weight: 500;
  color: var(--mm-ink);
}

.docs-bar__menu::after {
  width: 1px;
  height: 1.1rem;
  margin: 0 -0.4rem 0 0.35rem;
  content: "";
  background: var(--mm-line-strong);
}

.docs-bar__outline {
  flex: 1;
  min-width: 0;
  margin: 0;
  text-align: left;
}

.docs-bar__outline--static {
  cursor: default;
}

.docs-bar__menu:hover,
button.docs-bar__outline:hover {
  color: var(--mm-ink);
  background: rgb(255 255 255 / 6%);
}

.docs-bar__label {
  flex: none;
  font-family: var(--mm-font-mono);
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.12em;
  color: var(--mm-muted);
  text-transform: uppercase;
}

.docs-bar__section {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--mm-ink);
  white-space: nowrap;
}

.docs-bar__chevron {
  flex: none;
  color: var(--mm-muted);
  transition: transform 0.25s var(--mm-ease);
}

.docs-bar__outline[data-state="open"] .docs-bar__chevron {
  transform: rotate(180deg);
}

.docs-bar__search,
.docs-bar__search:focus-visible,
.docs-bar__menu:focus-visible,
.docs-bar__outline:focus-visible {
  border-radius: var(--docs-bar-inner-radius);
}

.docs-bar__search {
  flex: none;
  width: calc(var(--docs-bar-h) - 2 * var(--docs-bar-gap));
  height: calc(var(--docs-bar-h) - 2 * var(--docs-bar-gap));
  background: none;
  border-color: transparent;
  box-shadow: none;
}

.docs-bar :is(.docs-bar__menu, .docs-bar__outline, .docs-bar__search):focus-visible {
  outline-offset: -2px;
}

/* The outline drops down over the page instead of pushing it. */
.docs-bar__panel {
  position: absolute;
  inset: 100% -1px auto;
  overflow: hidden;
  background: var(--mm-surface);
  border: 1px solid var(--mm-line-strong);
  border-top-color: var(--mm-line);
  border-radius: 0 0 var(--docs-bar-radius) var(--docs-bar-radius);
  box-shadow: 0 24px 50px -18px rgb(0 0 0 / 85%);
}

.docs-bar[data-state="open"] {
  border-bottom-color: transparent;
  border-radius: var(--docs-bar-radius) var(--docs-bar-radius) 0 0;
}

.docs-bar__panel[data-state="open"] {
  animation: docs-bar-open 0.24s var(--mm-ease-out);
}

.docs-bar__panel[data-state="closed"] {
  animation: docs-bar-open 0.16s ease-in reverse;
}

.docs-bar__toc {
  max-height: min(60vh, 26rem);
  padding: 0.75rem 1rem 0.9rem;
  overflow-y: auto;
  overscroll-behavior: contain;
}

@keyframes docs-bar-open {
  from {
    height: 0;
    opacity: 0;
  }

  to {
    height: var(--reka-collapsible-content-height);
  }
}

/* With the sidebar on screen the bar only carries the outline. */
@media (min-width: 900.02px) {
  .docs-bar {
    margin-inline: 0;
  }

  .docs-bar__menu,
  .docs-bar__search {
    display: none;
  }

  .docs-bar__outline {
    padding-left: 0.85rem;
  }
}

@media (max-width: 480px) {
  button.docs-bar__outline .docs-bar__section {
    visibility: hidden;
  }

  .docs-bar__outline--static .docs-bar__label {
    display: none;
  }
}
</style>
