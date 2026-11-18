<script setup lang="ts">
/** A heading in the article with a link to itself that shows on hover or focus. */
defineProps<{ level: 2 | 3 | 4 | 5 | 6; id?: string }>();
</script>

<template>
  <component :is="`h${level}`" :id="id" class="docs-heading">
    <slot />
    <a v-if="id" :href="`#${id}`" class="docs-heading__anchor" aria-label="Link to this section">
      <span aria-hidden="true">#</span>
    </a>
  </component>
</template>

<style>
.docs-heading__anchor {
  display: inline-block;
  margin-left: 0.35em;
  padding: 0 0.15em;
  font-family: var(--mm-font-mono);
  font-size: 0.8em;
  font-weight: 500;
  color: var(--mm-faint);
  text-decoration: none;
  border-radius: 4px;
  opacity: 0;
  transition:
    opacity 0.15s var(--mm-ease),
    color 0.15s var(--mm-ease);
}

.docs-heading:hover .docs-heading__anchor,
.docs-heading__anchor:focus-visible {
  opacity: 1;
}

.docs-heading__anchor:hover {
  color: var(--mm-accent);
}

/* Nothing to hover on a touch screen, so the marker stays faintly visible. */
@media (hover: none) {
  .docs-heading__anchor {
    opacity: 0.6;
  }
}
</style>
