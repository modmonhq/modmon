<script setup lang="ts">
import { ToggleGroupItem, ToggleGroupRoot } from "reka-ui";

/** A single choice from a short row of options. One option is always selected. */
const value = defineModel<string>({ required: true });
defineProps<{ options: readonly { value: string; label: string }[]; label: string }>();

// Reka clears the value when the active item is pressed again; a choice stays chosen.
function choose(next: unknown) {
  if (typeof next === "string" && next) value.value = next;
}
</script>

<template>
  <ToggleGroupRoot
    type="single"
    class="mm-segmented"
    :model-value="value"
    :aria-label="label"
    @update:model-value="choose"
  >
    <ToggleGroupItem
      v-for="option in options"
      :key="option.value"
      :value="option.value"
      class="mm-segmented__item"
    >
      {{ option.label }}
    </ToggleGroupItem>
  </ToggleGroupRoot>
</template>

<style>
.mm-segmented {
  --mm-segmented-radius: 8px;
  --mm-segmented-pad: 2px;

  display: inline-flex;
  flex: none;
  gap: 1px;
  padding: var(--mm-segmented-pad);
  background: rgb(255 255 255 / 4%);
  border: 1px solid var(--mm-line);
  border-radius: var(--mm-segmented-radius);
}

.mm-segmented__item {
  padding: 0.28rem 0.6rem;
  font-family: var(--mm-font-mono);
  font-size: var(--mm-text-xs);
  font-weight: 500;
  line-height: 1.2;
  color: var(--mm-muted);
  cursor: pointer;
  background: none;
  border: 0;
  /* Concentric with the group: its radius less the padding and border between the edges. */
  border-radius: max(0px, calc(var(--mm-segmented-radius) - var(--mm-segmented-pad) - 1px));
  transition:
    color 0.2s var(--mm-ease),
    background-color 0.2s var(--mm-ease);
}

.mm-segmented__item:hover {
  color: var(--mm-ink);
}

.mm-segmented__item[data-state="on"] {
  color: var(--mm-accent-ink);
  background: var(--mm-accent);
}

.mm-segmented__item:focus-visible {
  outline-offset: 1px;
}
</style>
