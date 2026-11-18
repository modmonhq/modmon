<script setup lang="ts">
import type { Component } from "vue";
import { Primitive } from "reka-ui";

/** A link that leaves the page, marked with the accent `↗` the terminal puts before a URL. */
withDefaults(defineProps<{ as?: string | Component }>(), { as: "a" });
</script>

<template>
  <Primitive :as="as" class="mm-arrow-link">
    <svg class="mm-arrow-link__arrow" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <path d="M4.5 11.5 11.5 4.5M5.5 4.5h6v6" />
    </svg>
    <span class="mm-arrow-link__label"><slot /></span>
  </Primitive>
</template>

<style>
.mm-arrow-link {
  display: inline-flex;
  gap: 0.45em;
  align-items: center;
  min-width: 0;
  max-width: 100%;
  color: var(--mm-ink);
  text-decoration: none;
}

.mm-arrow-link__arrow {
  flex: none;
  width: 0.95em;
  height: 0.95em;
  fill: none;
  stroke: var(--mm-accent);
  stroke-width: 1.7;
  stroke-linecap: round;
  stroke-linejoin: round;
  transition: transform 0.2s var(--mm-ease);
}

.mm-arrow-link__label {
  min-width: 0;
  overflow: hidden;
  text-decoration: underline;
  text-decoration-color: transparent;
  text-underline-offset: 0.22em;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition:
    color 0.2s var(--mm-ease),
    text-decoration-color 0.2s var(--mm-ease);
}

.mm-arrow-link:hover .mm-arrow-link__label {
  color: var(--mm-accent-strong);
  text-decoration-color: rgb(var(--mm-accent-rgb) / 55%);
}

.mm-arrow-link:hover .mm-arrow-link__arrow {
  transform: translate(0.1em, -0.1em);
}
</style>
