<script setup lang="ts">
import type { Tone } from "../types";
import MmBadge from "./MmBadge.vue";

/** A centered message for a panel with nothing to show; the default slot adds a line of detail. */
withDefaults(defineProps<{ title: string; tone?: Tone }>(), { tone: "idle" });
</script>

<template>
  <div class="mm-empty">
    <MmBadge :tone="tone">{{ title }}</MmBadge>
    <p v-if="$slots.default" class="mm-empty__detail"><slot /></p>
  </div>
</template>

<style>
.mm-empty {
  display: flex;
  flex-direction: column;
  gap: 0.6em;
  align-items: center;
  justify-content: center;
  min-height: 9em;
  padding: 1.5em 1em;
  text-align: center;
}

.mm-empty__detail {
  max-width: 44ch;
  margin: 0;
  color: var(--mm-muted);
  text-wrap: balance;
}

.mm-empty__detail code {
  color: var(--mm-ink-soft);
}
</style>
