<script setup lang="ts">
import type { Component } from "vue";
import { Primitive } from "reka-ui";

withDefaults(
  defineProps<{
    variant?: "primary" | "secondary" | "ghost";
    size?: "sm" | "md" | "lg";
    /** Render as a link (`a`), a router link component, or a button. */
    as?: string | Component;
    /** Merge the button onto its only child instead of rendering an element. */
    asChild?: boolean;
  }>(),
  { variant: "secondary", size: "md", as: "button" },
);
</script>

<template>
  <Primitive
    :as="as"
    :as-child="asChild"
    class="mm-button"
    :class="[`mm-button--${variant}`, `mm-button--${size}`]"
    :type="as === 'button' && !asChild ? 'button' : undefined"
  >
    <slot />
  </Primitive>
</template>

<style>
.mm-button {
  --mm-button-h: 2.5rem;

  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.55em;
  height: var(--mm-button-h);
  padding: 0 1.05em;
  font-family: var(--mm-font-sans);
  font-size: var(--mm-text-sm);
  font-weight: 600;
  line-height: 1;
  letter-spacing: 0.005em;
  color: var(--mm-ink);
  text-decoration: none;
  white-space: nowrap;
  cursor: pointer;
  background: var(--mm-surface-2);
  border: 1px solid var(--mm-line-strong);
  border-radius: 9px;
  transition:
    background-color 0.2s var(--mm-ease),
    border-color 0.2s var(--mm-ease),
    color 0.2s var(--mm-ease),
    box-shadow 0.25s var(--mm-ease),
    transform 0.2s var(--mm-ease);
}

.mm-button:active {
  transform: translateY(1px);
}

.mm-button--sm {
  --mm-button-h: 2.125rem;

  padding: 0 0.8em;
  font-size: 0.8125rem;
}

.mm-button--lg {
  --mm-button-h: 3rem;

  padding: 0 1.35em;
  font-size: var(--mm-text-md);
  border-radius: 11px;
}

.mm-button--primary {
  color: var(--mm-accent-ink);
  background: linear-gradient(180deg, var(--mm-accent-strong), var(--mm-accent) 55%, #b4e23a);
  border-color: transparent;
  box-shadow:
    inset 0 1px 0 rgb(255 255 255 / 55%),
    inset 0 -1px 0 rgb(0 0 0 / 18%),
    0 0 0 1px rgb(var(--mm-accent-rgb) / 35%),
    0 10px 30px -10px rgb(var(--mm-accent-rgb) / 55%);
}

.mm-button--primary:hover {
  box-shadow:
    inset 0 1px 0 rgb(255 255 255 / 65%),
    inset 0 -1px 0 rgb(0 0 0 / 18%),
    0 0 0 1px rgb(var(--mm-accent-rgb) / 55%),
    0 14px 40px -8px rgb(var(--mm-accent-rgb) / 70%);
}

.mm-button--secondary {
  background: linear-gradient(180deg, rgb(255 255 255 / 7%), rgb(255 255 255 / 3%));
  box-shadow: inset 0 1px 0 rgb(255 255 255 / 6%);
}

.mm-button--secondary:hover {
  background: linear-gradient(180deg, rgb(255 255 255 / 11%), rgb(255 255 255 / 5%));
  border-color: rgb(232 246 214 / 26%);
}

.mm-button--ghost {
  color: var(--mm-ink-soft);
  background: transparent;
  border-color: transparent;
}

.mm-button--ghost:hover {
  color: var(--mm-ink);
  background: rgb(255 255 255 / 6%);
}
</style>
