<script setup lang="ts">
import { MARK_HEIGHT, MARK_LETTERS, MARK_RADIUS, MARK_WIDTH } from "../brand";

/** The solid `mm` block from the terminal header, optionally followed by a name. */
withDefaults(defineProps<{ name?: string }>(), { name: undefined });
</script>

<template>
  <span class="mm-wordmark">
    <svg class="mm-wordmark__mark" :viewBox="`0 0 ${MARK_WIDTH} ${MARK_HEIGHT}`" aria-hidden="true">
      <rect :width="MARK_WIDTH" :height="MARK_HEIGHT" :rx="MARK_RADIUS" fill="var(--mm-accent)" />
      <path :d="MARK_LETTERS" fill="var(--mm-accent-ink)" />
    </svg>
    <span v-if="name" class="mm-wordmark__name">{{ name }}</span>
    <span v-else class="mm-sr-only">mm</span>
  </span>
</template>

<style>
.mm-wordmark {
  display: inline-flex;
  align-items: center;
  gap: 0.6em;
  color: var(--mm-ink);
  text-decoration: none;
}

.mm-wordmark__mark {
  flex: none;
  width: auto;
  height: 1.5em;
  filter: drop-shadow(0 0 0.55em rgb(var(--mm-accent-rgb) / 35%));
}

.mm-wordmark__name {
  font-family: var(--mm-font-display);
  font-size: 1.12em;
  font-weight: 600;
  letter-spacing: 0.01em;
}
</style>
