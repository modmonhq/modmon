<script setup lang="ts">
import { SwitchRoot, SwitchThumb } from "reka-ui";

const checked = defineModel<boolean>({ default: false });
defineProps<{ disabled?: boolean }>();
</script>

<template>
  <SwitchRoot v-model="checked" class="mm-switch" :disabled="disabled">
    <SwitchThumb class="mm-switch__thumb" />
  </SwitchRoot>
</template>

<style>
.mm-switch {
  position: relative;
  flex: none;
  width: 2.25rem;
  height: 1.3rem;
  padding: 0;
  cursor: pointer;
  background: rgb(255 255 255 / 7%);
  border: 1px solid var(--mm-line-strong);
  border-radius: 999px;
  transition:
    background-color 0.2s var(--mm-ease),
    border-color 0.2s var(--mm-ease),
    box-shadow 0.2s var(--mm-ease);
}

.mm-switch[data-state="checked"] {
  background: rgb(var(--mm-accent-rgb) / 22%);
  border-color: rgb(var(--mm-accent-rgb) / 60%);
  box-shadow: 0 0 14px -2px rgb(var(--mm-accent-rgb) / 45%);
}

.mm-switch[data-disabled] {
  cursor: not-allowed;
  opacity: 0.7;
}

.mm-switch__thumb {
  display: block;
  width: 0.9rem;
  height: 0.9rem;
  background: var(--mm-muted);
  border-radius: 50%;
  transform: translateX(0.15rem);
  transition:
    transform 0.22s var(--mm-ease-out),
    background-color 0.2s var(--mm-ease);
}

.mm-switch[data-state="checked"] .mm-switch__thumb {
  background: var(--mm-accent);
  transform: translateX(1.1rem);
}
</style>
