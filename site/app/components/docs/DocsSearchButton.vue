<script setup lang="ts">
/** Opens the search palette. Looks like a field so it reads as "type here". */
withDefaults(defineProps<{ compact?: boolean }>(), { compact: false });
</script>

<template>
  <button type="button" class="docs-search-button" :class="{ 'docs-search-button--compact': compact }">
    <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
      <circle cx="7" cy="7" r="4.4" fill="none" stroke="currentColor" stroke-width="1.5" />
      <path
        d="m10.4 10.4 3.1 3.1"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
      />
    </svg>
    <span class="docs-search-button__label" :class="{ 'mm-sr-only': compact }">Search docs</span>
    <kbd v-if="!compact" class="docs-search-button__key" aria-hidden="true">/</kbd>
  </button>
</template>

<style scoped>
.docs-search-button {
  /* The key hint sits --field-gap inside the border; its radius follows from that. */
  --field-h: 2.375rem;
  --field-radius: var(--mm-radius);
  --field-gap: 5px;

  display: flex;
  gap: 0.6rem;
  align-items: center;
  width: 100%;
  height: var(--field-h);
  padding: 0 var(--field-gap) 0 0.75rem;
  font-family: var(--mm-font-sans);
  font-size: var(--mm-text-sm);
  color: var(--mm-muted);
  text-align: left;
  cursor: pointer;
  background: rgb(255 255 255 / 2.5%);
  border: 1px solid var(--mm-line-strong);
  border-radius: var(--field-radius);
  box-shadow: inset 0 1px 0 rgb(255 255 255 / 4%);
  transition:
    color 0.2s var(--mm-ease),
    border-color 0.2s var(--mm-ease),
    background-color 0.2s var(--mm-ease);
}

.docs-search-button:hover {
  color: var(--mm-ink-soft);
  background: rgb(255 255 255 / 5%);
  border-color: rgb(232 246 214 / 26%);
}

.docs-search-button:focus-visible {
  border-radius: var(--field-radius);
}

.docs-search-button svg {
  flex: none;
}

.docs-search-button__label {
  flex: 1;
}

.docs-search-button__key {
  display: grid;
  flex: none;
  place-items: center;
  min-width: calc(var(--field-h) - 2px - 2 * var(--field-gap));
  height: calc(var(--field-h) - 2px - 2 * var(--field-gap));
  padding: 0 0.3rem;
  font-size: 0.75rem;
  font-weight: 600;
  line-height: 1;
  color: var(--mm-accent);
  background: rgb(var(--mm-accent-rgb) / 8%);
  border: 1px solid rgb(var(--mm-accent-rgb) / 22%);
  border-radius: calc(var(--field-radius) - 1px - var(--field-gap));
}

.docs-search-button--compact {
  justify-content: center;
  width: var(--field-h);
  padding: 0;
}
</style>
