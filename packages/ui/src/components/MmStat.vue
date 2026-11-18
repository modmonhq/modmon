<script setup lang="ts">
import type { Tone } from "../types";
import MmCard from "./MmCard.vue";
import MmTone from "./MmTone.vue";

/**
 * A headline figure in a titled card: tone glyph and value on one line, a line of detail
 * beneath. Matches the summary cards at the top of the terminal overview.
 */
withDefaults(
  defineProps<{
    title: string;
    tone: Tone;
    value: string | number;
    detail?: string;
    headingLevel?: 2 | 3 | 4;
  }>(),
  { detail: undefined, headingLevel: undefined },
);
</script>

<template>
  <MmCard class="mm-stat" :title="title" :heading-level="headingLevel">
    <p class="mm-stat__value">
      <MmTone :tone="tone" /><span class="mm-stat__figure">{{ value }}</span>
    </p>
    <p v-if="detail || $slots.default" class="mm-stat__detail">
      <slot>{{ detail }}</slot>
    </p>
  </MmCard>
</template>

<style>
.mm-stat__value {
  display: flex;
  gap: 0.45em;
  align-items: center;
  min-width: 0;
  margin: 0;
  font-size: 1.5em;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  line-height: 1.3;
  color: var(--mm-ink);
}

.mm-stat__value .mm-tone {
  font-size: 0.66em;
}

.mm-stat__figure {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mm-stat__detail {
  margin: 0.15em 0 0;
  color: var(--mm-muted);
  overflow-wrap: anywhere;
}
</style>
