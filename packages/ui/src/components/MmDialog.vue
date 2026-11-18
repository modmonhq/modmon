<script setup lang="ts">
import {
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from "reka-ui";
import MmCard from "./MmCard.vue";

/**
 * A modal panel: the terminal interface's titled card over a dimmed page. Focus is trapped
 * while it is open and returns to whatever opened it. `description` is read by screen
 * readers only.
 */
withDefaults(
  defineProps<{ title: string; description: string; placement?: "center" | "top"; width?: string }>(),
  { placement: "center", width: "34rem" },
);

const open = defineModel<boolean>("open", { default: false });
</script>

<template>
  <DialogRoot v-model:open="open">
    <DialogPortal>
      <DialogOverlay class="mm-dialog__overlay" />
      <DialogContent
        as-child
        class="mm-dialog"
        :class="`mm-dialog--${placement}`"
        :style="{ '--mm-dialog-width': width }"
      >
        <MmCard as="div" focused>
          <template #title>
            <DialogTitle as="span">{{ title }}</DialogTitle>
          </template>
          <template v-if="$slots.meta" #meta><slot name="meta" /></template>
          <DialogDescription class="mm-sr-only">{{ description }}</DialogDescription>
          <slot />
        </MmCard>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

<style>
.mm-dialog__overlay {
  position: fixed;
  inset: 0;
  z-index: 300;
  background: rgb(4 5 4 / 72%);
  backdrop-filter: blur(6px);
}

.mm-dialog {
  --mm-card-pad: 1.1rem;
  --mm-dialog-fill: var(--mm-surface);
  --mm-card-surface: var(--mm-dialog-fill);

  position: fixed;
  left: 50%;
  z-index: 301;
  width: min(var(--mm-dialog-width), calc(100vw - 1.5rem));
  max-height: min(42rem, calc(100dvh - 2rem));
  font-family: var(--mm-font-mono);
  font-size: var(--mm-text-sm);
  filter: drop-shadow(0 30px 60px rgb(0 0 0 / 70%));
  transform: translateX(-50%);
}

.mm-dialog:focus-visible {
  outline: none;
}

.mm-dialog--center {
  top: 50%;
  transform: translate(-50%, -50%);
}

.mm-dialog--top {
  top: min(14vh, 8rem);
  max-height: min(42rem, calc(100dvh - min(14vh, 8rem) - 1rem));
}

/* A fixed height, so the card can place its border through the middle of the title. */
.mm-dialog > .mm-card__top {
  flex: none;
  height: 1.5em;
}

.mm-dialog > .mm-card__body {
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 0;
}

.mm-dialog__overlay[data-state="open"] {
  animation: mm-dialog-fade 0.18s var(--mm-ease-out);
}

.mm-dialog__overlay[data-state="closed"] {
  animation: mm-dialog-fade 0.12s ease-in reverse;
}

.mm-dialog[data-state="open"] {
  animation: mm-dialog-in 0.22s var(--mm-ease-out);
}

.mm-dialog[data-state="closed"] {
  animation: mm-dialog-fade 0.12s ease-in reverse;
}

@keyframes mm-dialog-fade {
  from {
    opacity: 0;
  }
}

@keyframes mm-dialog-in {
  from {
    opacity: 0;
    translate: 0 0.5rem;
    scale: 0.985;
  }
}
</style>
