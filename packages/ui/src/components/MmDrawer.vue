<script setup lang="ts">
import {
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from "reka-ui";

/**
 * A modal sheet that slides in from the edge of the screen, for navigation on small
 * screens. `description` is read by screen readers only.
 */
withDefaults(
  defineProps<{ title: string; description: string; side?: "left" | "right"; closeLabel?: string }>(),
  { side: "left", closeLabel: "Close" },
);

const open = defineModel<boolean>("open", { default: false });
</script>

<template>
  <DialogRoot v-model:open="open">
    <DialogPortal>
      <DialogOverlay class="mm-drawer__overlay" />
      <DialogContent class="mm-drawer" :class="`mm-drawer--${side}`">
        <div class="mm-drawer__head">
          <DialogTitle class="mm-drawer__title">{{ title }}</DialogTitle>
          <DialogDescription class="mm-sr-only">{{ description }}</DialogDescription>
          <DialogClose class="mm-drawer__close">
            <kbd aria-hidden="true">esc</kbd>
            <span>{{ closeLabel }}</span>
          </DialogClose>
        </div>
        <div class="mm-drawer__body"><slot /></div>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

<style>
.mm-drawer__overlay {
  position: fixed;
  inset: 0;
  z-index: 300;
  background: rgb(4 5 4 / 72%);
  backdrop-filter: blur(6px);
}

.mm-drawer {
  --mm-drawer-shift: -100%;

  position: fixed;
  inset: 0 auto 0 0;
  z-index: 301;
  display: flex;
  flex-direction: column;
  width: min(21rem, calc(100vw - 3rem));
  background: var(--mm-surface);
  border-right: 1px solid var(--mm-line-strong);
  box-shadow: 30px 0 80px -20px rgb(0 0 0 / 80%);
}

.mm-drawer--right {
  --mm-drawer-shift: 100%;

  inset: 0 0 0 auto;
  border-right: 0;
  border-left: 1px solid var(--mm-line-strong);
  box-shadow: -30px 0 80px -20px rgb(0 0 0 / 80%);
}

.mm-drawer:focus-visible {
  outline: none;
}

.mm-drawer__head {
  display: flex;
  flex: none;
  gap: 1rem;
  align-items: center;
  justify-content: space-between;
  height: 3.5rem;
  padding: 0 0.75rem 0 1.25rem;
  border-bottom: 1px solid var(--mm-line);
}

.mm-drawer__title {
  margin: 0;
  font-family: var(--mm-font-mono);
  font-size: var(--mm-text-xs);
  font-weight: 600;
  letter-spacing: 0.14em;
  color: var(--mm-ink-soft);
  text-transform: uppercase;
}

.mm-drawer__close {
  display: inline-flex;
  gap: 0.6em;
  align-items: center;
  height: 2.25rem;
  padding: 0 0.6rem;
  font-family: var(--mm-font-mono);
  font-size: var(--mm-text-xs);
  color: var(--mm-muted);
  cursor: pointer;
  background: none;
  border: 0;
  border-radius: var(--mm-radius-sm);
  transition:
    color 0.2s var(--mm-ease),
    background-color 0.2s var(--mm-ease);
}

.mm-drawer__close:hover {
  color: var(--mm-ink);
  background: rgb(255 255 255 / 6%);
}

.mm-drawer__close kbd {
  font: inherit;
  font-weight: 700;
  color: var(--mm-accent);
}

.mm-drawer__body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
}

.mm-drawer__overlay[data-state="open"] {
  animation: mm-drawer-fade 0.2s var(--mm-ease-out);
}

.mm-drawer__overlay[data-state="closed"] {
  animation: mm-drawer-fade 0.16s ease-in reverse;
}

.mm-drawer[data-state="open"] {
  animation: mm-drawer-in 0.28s var(--mm-ease-out);
}

.mm-drawer[data-state="closed"] {
  animation: mm-drawer-in 0.18s ease-in reverse;
}

@keyframes mm-drawer-fade {
  from {
    opacity: 0;
  }
}

@keyframes mm-drawer-in {
  from {
    transform: translateX(var(--mm-drawer-shift));
  }
}
</style>
