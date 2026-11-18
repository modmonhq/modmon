<script setup lang="ts">
/** A terminal window: hairline frame, quiet title bar, and a slot for what runs inside. */
withDefaults(defineProps<{ title?: string }>(), { title: undefined });
</script>

<template>
  <div class="mm-window">
    <div class="mm-window__bar">
      <span class="mm-window__dots" aria-hidden="true"><i /><i /><i /></span>
      <span v-if="title || $slots.title" class="mm-window__title">
        <slot name="title">{{ title }}</slot>
      </span>
      <span class="mm-window__aside"><slot name="aside" /></span>
    </div>
    <div class="mm-window__body"><slot /></div>
  </div>
</template>

<style>
.mm-window {
  --mm-window-bg: #090b0a;
  --mm-window-radius: var(--mm-radius-lg);

  position: relative;
  display: flex;
  flex-direction: column;
  min-width: 0;
  background: var(--mm-window-bg);
  border-radius: var(--mm-window-radius);
  box-shadow:
    0 0 0 1px var(--mm-line-strong),
    inset 0 1px 0 rgb(255 255 255 / 7%);
}

.mm-window__bar {
  display: grid;
  flex: none;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  height: 2.5rem;
  padding: 0 0.95rem;
  background: linear-gradient(180deg, rgb(255 255 255 / 5%), rgb(255 255 255 / 1.5%));
  border-bottom: 1px solid var(--mm-line);
  border-radius: var(--mm-window-radius) var(--mm-window-radius) 0 0;
}

.mm-window__dots {
  display: flex;
  gap: 0.44rem;
}

.mm-window__dots i {
  width: 0.66rem;
  height: 0.66rem;
  background: rgb(255 255 255 / 12%);
  border-radius: 50%;
  box-shadow: inset 0 0 0 1px rgb(255 255 255 / 6%);
}

.mm-window__title {
  font-family: var(--mm-font-mono);
  font-size: var(--mm-text-xs);
  color: var(--mm-muted);
  white-space: nowrap;
}

.mm-window__aside {
  display: flex;
  grid-column: 3;
  justify-content: flex-end;
  min-width: 0;
}

.mm-window__body {
  min-width: 0;
  container-type: inline-size;
}
</style>
