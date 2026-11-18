<script setup lang="ts">
import { computed } from "vue";

/** One pip per desired replica: healthy, running but unhealthy, or missing. */
const props = withDefaults(
  defineProps<{ desired: number; running: number; healthy?: number; max?: number }>(),
  { healthy: undefined, max: 10 },
);

const pips = computed(() => {
  const healthy = props.healthy ?? props.running;
  return Array.from({ length: Math.min(props.desired, props.max) }, (_, index) =>
    index < healthy ? "ok" : index < props.running ? "warn" : "off",
  );
});
</script>

<template>
  <span class="mm-pips" role="img" :aria-label="`${running} of ${desired} running`">
    <i v-for="(state, index) in pips" :key="index" class="mm-pip" :class="`mm-pip--${state}`" />
    <span v-if="desired > max" class="mm-pips__more" aria-hidden="true">…</span>
  </span>
</template>

<style>
.mm-pips {
  display: inline-flex;
  align-items: center;
  gap: 0.22em;
  vertical-align: middle;
}

.mm-pip {
  width: 0.62em;
  height: 0.62em;
  border-radius: 1.5px;
  transform: skewX(-18deg);
  transition:
    background-color 0.35s var(--mm-ease),
    box-shadow 0.35s var(--mm-ease);
}

.mm-pip--ok {
  background: var(--mm-ok);
  box-shadow: 0 0 0.6em rgb(var(--mm-accent-rgb) / 45%);
}

.mm-pip--warn {
  background: var(--mm-warn);
}

.mm-pip--off {
  background: transparent;
  box-shadow: inset 0 0 0 1px var(--mm-muted);
}

.mm-pips__more {
  color: var(--mm-muted);
}
</style>
