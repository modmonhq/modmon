<script setup lang="ts">
import type { Tone } from "../types";
import MmTone from "./MmTone.vue";

withDefaults(defineProps<{ tone: Tone; variant?: "plain" | "pill"; pulse?: boolean }>(), {
  variant: "plain",
});
</script>

<template>
  <span class="mm-badge" :class="`mm-badge--${variant}`" :data-tone="tone">
    <MmTone :tone="tone" :pulse="pulse" />
    <span class="mm-badge__label"><slot /></span>
  </span>
</template>

<style>
.mm-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.5em;
  color: var(--tone);
  white-space: nowrap;
}

.mm-badge--pill {
  padding: 0.28em 0.75em 0.28em 0.6em;
  font-family: var(--mm-font-mono);
  font-size: var(--mm-text-xs);
  font-weight: 500;
  letter-spacing: 0.01em;
  background: rgb(var(--tone-rgb) / 9%);
  border: 1px solid rgb(var(--tone-rgb) / 24%);
  border-radius: 999px;
}
</style>
