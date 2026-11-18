<script setup lang="ts">
import type { Tone } from "../types";

defineProps<{ tone: Tone; pulse?: boolean }>();
</script>

<template>
  <svg
    class="mm-tone"
    :class="{ 'mm-tone--pulse': pulse }"
    :data-tone="tone"
    viewBox="0 0 16 16"
    aria-hidden="true"
    focusable="false"
  >
    <circle v-if="tone === 'ok'" cx="8" cy="8" r="4.2" fill="currentColor" />
    <path v-else-if="tone === 'warn'" d="M8 2.9 13.9 12.9H2.1Z" fill="currentColor" />
    <path
      v-else-if="tone === 'bad'"
      d="m3.8 3.8 8.4 8.4m0-8.4-8.4 8.4"
      fill="none"
      stroke="currentColor"
      stroke-width="2.2"
      stroke-linecap="round"
    />
    <path v-else-if="tone === 'info'" d="M8 2.6 13.4 8 8 13.4 2.6 8Z" fill="currentColor" />
    <circle v-else cx="8" cy="8" r="3.6" fill="none" stroke="currentColor" stroke-width="1.6" />
  </svg>
</template>

<style>
.mm-tone {
  flex: none;
  width: 1em;
  height: 1em;
  color: var(--tone);
  vertical-align: -0.14em;
  overflow: visible;
}

.mm-tone--pulse {
  animation: mm-tone-pulse 2.4s ease-in-out infinite;
  filter: drop-shadow(0 0 0.35em rgb(var(--tone-rgb) / 70%));
}

@keyframes mm-tone-pulse {
  50% {
    opacity: 0.55;
    filter: drop-shadow(0 0 0.1em rgb(var(--tone-rgb) / 30%));
  }
}
</style>
