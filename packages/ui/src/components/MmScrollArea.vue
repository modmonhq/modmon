<script setup lang="ts">
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from "reka-ui";

/**
 * A vertically scrolling region with a hairline scrollbar that only shows while scrolling
 * or hovering. Give it a height (or a flex parent that constrains it).
 */
</script>

<template>
  <ScrollAreaRoot class="mm-scroll" type="hover" :scroll-hide-delay="400">
    <ScrollAreaViewport class="mm-scroll__viewport">
      <slot />
    </ScrollAreaViewport>
    <ScrollAreaScrollbar class="mm-scroll__bar" orientation="vertical">
      <ScrollAreaThumb class="mm-scroll__thumb" />
    </ScrollAreaScrollbar>
  </ScrollAreaRoot>
</template>

<style>
.mm-scroll {
  position: relative;
  min-height: 0;
  overflow: hidden;
}

.mm-scroll__viewport {
  width: 100%;
  height: 100%;
  overscroll-behavior: contain;
}

.mm-scroll__bar {
  display: flex;
  width: 9px;
  padding: 3px;
  touch-action: none;
  user-select: none;
}

.mm-scroll__bar[data-state="visible"] {
  animation: mm-scroll-fade 0.18s var(--mm-ease-out);
}

.mm-scroll__bar[data-state="hidden"] {
  animation: mm-scroll-fade 0.25s ease-in reverse;
}

.mm-scroll__thumb {
  position: relative;
  flex: 1;
  background: rgb(232 246 214 / 18%);
  border-radius: 999px;
  transition: background-color 0.2s var(--mm-ease);
}

.mm-scroll__thumb:hover {
  background: rgb(232 246 214 / 34%);
}

@keyframes mm-scroll-fade {
  from {
    opacity: 0;
  }
}
</style>
