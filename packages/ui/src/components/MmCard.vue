<script setup lang="ts">
import type { Tone } from "../types";

/**
 * A rounded panel with its title set into the top border and an optional right-aligned
 * summary, matching the cards in the terminal interface.
 *
 * Styling hooks (CSS custom properties on the card):
 *   --mm-card-radius   corner radius of the frame and the surface
 *   --mm-card-pad      inner padding; content inside should use a radius of
 *                      `--mm-card-radius - --mm-card-pad` (or 0) to stay concentric
 *   --mm-card-line     border color
 *   --mm-card-surface  background painted inside the frame
 *   --mm-card-row      height of the title row; the border runs through its middle
 */
withDefaults(
  defineProps<{ title?: string; focused?: boolean; tone?: Tone; as?: string; headingLevel?: 2 | 3 | 4 }>(),
  { title: undefined, tone: undefined, as: "section", headingLevel: undefined },
);
</script>

<template>
  <component
    :is="as"
    class="mm-card"
    :class="{ 'mm-card--focused': focused, 'mm-card--toned': tone }"
    :data-tone="tone"
  >
    <span class="mm-card__corner mm-card__corner--start" aria-hidden="true" />
    <span class="mm-card__corner mm-card__corner--end" aria-hidden="true" />
    <span class="mm-card__frame" aria-hidden="true" />
    <div class="mm-card__top">
      <component
        :is="headingLevel ? `h${headingLevel}` : 'span'"
        v-if="title || $slots.title"
        class="mm-card__title"
      >
        <slot name="title">{{ title }}</slot>
      </component>
      <span class="mm-card__rule" />
      <span v-if="$slots.meta" class="mm-card__meta"><slot name="meta" /></span>
    </div>
    <div class="mm-card__body"><slot /></div>
  </component>
</template>

<style>
.mm-card {
  --mm-card-line: var(--mm-border);
  --mm-card-radius: var(--mm-radius);
  --mm-card-pad: 1rem;
  --mm-card-row: 1.5em;
  --mm-card-surface: transparent;

  position: relative;
  display: flex;
  flex-direction: column;
  min-width: 0;
  isolation: isolate;
}

.mm-card--focused {
  --mm-card-line: var(--mm-accent);
}

.mm-card--toned {
  --mm-card-line: var(--tone);
}

/* The surface starts at the border line and shares its radius, so the two never disagree. */
.mm-card::before {
  position: absolute;
  inset: calc(var(--mm-card-row) / 2) 0 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--mm-card-surface);
  border-radius: var(--mm-card-radius);
}

/*
 * The frame is drawn in pieces so the title can interrupt the top border over any
 * background: two corners as tall as the radius (a shorter piece would make the browser
 * shrink the curve), a rule between title and summary, and the sides and bottom.
 */
.mm-card__corner,
.mm-card__frame {
  position: absolute;
  pointer-events: none;
  transition: border-color 0.3s var(--mm-ease);
}

.mm-card__corner {
  top: calc(var(--mm-card-row) / 2);
  width: max(var(--mm-card-radius), var(--mm-card-pad));
  height: var(--mm-card-radius);
  border-top: 1px solid var(--mm-card-line);
}

.mm-card__corner--start {
  left: 0;
  border-left: 1px solid var(--mm-card-line);
  border-top-left-radius: var(--mm-card-radius);
}

.mm-card__corner--end {
  right: 0;
  border-right: 1px solid var(--mm-card-line);
  border-top-right-radius: var(--mm-card-radius);
}

.mm-card__frame {
  inset: calc(var(--mm-card-row) / 2 + var(--mm-card-radius)) 0 0;
  border: 1px solid var(--mm-card-line);
  border-top: 0;
  border-radius: 0 0 var(--mm-card-radius) var(--mm-card-radius);
}

.mm-card__top {
  display: flex;
  flex: none;
  align-items: stretch;
  /* A definite height, so the rule's 50% resolves and the border meets the corners. */
  height: var(--mm-card-row);
  padding-inline: max(var(--mm-card-radius), var(--mm-card-pad));
  line-height: var(--mm-card-row);
}

.mm-card__rule {
  flex: 1;
  align-self: flex-end;
  min-width: 0.5em;
  height: 50%;
  border-top: 1px solid var(--mm-card-line);
  transition: border-color 0.3s var(--mm-ease);
}

.mm-card__title,
.mm-card__meta {
  flex: none;
  max-width: 100%;
  margin: 0;
  padding: 0 0.6em;
  font: inherit;
  white-space: nowrap;
}

.mm-card__title {
  font-weight: 700;
  color: var(--mm-ink);
}

.mm-card__meta {
  color: var(--mm-muted);
}

.mm-card__body {
  flex: 1;
  min-width: 0;
  padding: calc(var(--mm-card-pad) * 0.35) var(--mm-card-pad) calc(var(--mm-card-pad) * 0.8);
}
</style>
