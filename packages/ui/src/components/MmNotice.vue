<script setup lang="ts">
import type { Tone } from "../types";
import MmTone from "./MmTone.vue";

/** An inline message that wraps: tone glyph, a short title, and detail in the default slot. */
withDefaults(defineProps<{ tone: Tone; title?: string }>(), { title: undefined });
</script>

<template>
  <div class="mm-notice" :data-tone="tone">
    <MmTone :tone="tone" />
    <p class="mm-notice__text">
      <strong v-if="title" class="mm-notice__title">{{ title }}</strong>
      <span v-if="$slots.default" class="mm-notice__detail"><slot /></span>
    </p>
  </div>
</template>

<style>
.mm-notice {
  display: flex;
  gap: 0.7em;
  align-items: baseline;
  min-width: 0;
  padding: 0.6em 0.9em;
  background: rgb(var(--tone-rgb) / 6%);
  border: 1px solid rgb(var(--tone-rgb) / 26%);
  border-radius: var(--mm-radius-sm);
}

.mm-notice .mm-tone {
  position: relative;
  top: 0.14em;
}

.mm-notice__text {
  display: flex;
  flex-wrap: wrap;
  gap: 0 0.9em;
  min-width: 0;
  margin: 0;
}

.mm-notice__title {
  font-weight: 600;
  color: var(--tone);
}

.mm-notice__detail {
  min-width: 0;
  color: var(--mm-ink-soft);
  overflow-wrap: anywhere;
}
</style>
