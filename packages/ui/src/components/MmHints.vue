<script setup lang="ts">
import type { Hint } from "../types";

defineProps<{ items: readonly Hint[] }>();
</script>

<template>
  <span class="mm-hints">
    <span v-for="[key, label] in items" :key="key" class="mm-hint">
      <kbd>{{ key }}</kbd> {{ label }}
    </span>
  </span>
</template>

<style>
.mm-hints {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 0 1.8em;
  font-family: var(--mm-font-mono);
}

.mm-hint {
  color: var(--mm-muted);
  white-space: nowrap;
}

.mm-hint kbd {
  font: inherit;
  font-weight: 700;
  color: var(--mm-accent);
}
</style>
