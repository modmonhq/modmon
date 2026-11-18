<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { TabsContent, TabsIndicator, TabsList, TabsRoot, TabsTrigger } from "reka-ui";
import type { Tone } from "../types";

/** Tabs with a sliding indicator. Each tab's panel is the slot named after its `value`. */
const active = defineModel<string>();
defineProps<{
  /** `count` is an optional figure after the label; `tone` colors it. */
  tabs: readonly { value: string; label: string; count?: number | string; tone?: Tone }[];
  label?: string;
}>();

/*
 * A list wider than its column scrolls sideways. When the tab is chosen from elsewhere on the
 * page, bring it into the middle of the list, without scrolling the page.
 */
const list = ref<{ $el: HTMLElement }>();
watch(active, async () => {
  await nextTick();
  const element = list.value?.$el;
  const trigger = element?.querySelector<HTMLElement>('[data-state="active"]');
  if (!element || !trigger || element.scrollWidth <= element.clientWidth) return;
  element.scrollTo({
    left: trigger.offsetLeft - (element.clientWidth - trigger.offsetWidth) / 2,
    behavior: "smooth",
  });
});
</script>

<template>
  <TabsRoot v-model="active" class="mm-tabs" :default-value="tabs[0]?.value">
    <TabsList ref="list" class="mm-tabs__list" :aria-label="label">
      <TabsIndicator class="mm-tabs__indicator" />
      <TabsTrigger v-for="tab in tabs" :key="tab.value" :value="tab.value" class="mm-tabs__trigger">
        {{ tab.label }}
        <span v-if="tab.count !== undefined" class="mm-tabs__count" :data-tone="tab.tone">{{
          tab.count
        }}</span>
      </TabsTrigger>
    </TabsList>
    <TabsContent v-for="tab in tabs" :key="tab.value" :value="tab.value" class="mm-tabs__panel">
      <slot :name="tab.value" />
    </TabsContent>
  </TabsRoot>
</template>

<style>
.mm-tabs {
  --mm-tabs-radius: var(--mm-radius);
  --mm-tabs-pad: 0.2rem;

  /* Concentric with the list: its radius less the padding and border between the edges. */
  --mm-tabs-inner-radius: max(0px, calc(var(--mm-tabs-radius) - var(--mm-tabs-pad) - 1px));
}

.mm-tabs__list {
  position: relative;
  display: inline-flex;
  gap: 0.15rem;
  padding: var(--mm-tabs-pad);
  background: rgb(255 255 255 / 4%);
  border: 1px solid var(--mm-line);
  border-radius: var(--mm-tabs-radius);
}

.mm-tabs__indicator {
  position: absolute;
  top: var(--mm-tabs-pad);
  bottom: var(--mm-tabs-pad);
  left: 0;
  width: var(--reka-tabs-indicator-size);
  background: rgb(255 255 255 / 9%);
  border-radius: var(--mm-tabs-inner-radius);
  box-shadow: inset 0 1px 0 rgb(255 255 255 / 8%);
  transform: translateX(var(--reka-tabs-indicator-position));
  transition:
    transform 0.28s var(--mm-ease-out),
    width 0.28s var(--mm-ease-out);
}

.mm-tabs__trigger {
  position: relative;
  padding: 0.4rem 0.9rem;
  font-family: var(--mm-font-mono);
  font-size: var(--mm-text-xs);
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--mm-muted);
  cursor: pointer;
  background: none;
  border: 0;
  border-radius: var(--mm-tabs-inner-radius);
  transition: color 0.2s var(--mm-ease);
}

.mm-tabs__trigger:hover,
.mm-tabs__trigger[data-state="active"] {
  color: var(--mm-ink);
}

.mm-tabs__count {
  margin-left: 0.35em;
  font-weight: 500;
  font-variant-numeric: tabular-nums;
  color: var(--tone, var(--mm-faint));
}

.mm-tabs__trigger[data-state="active"] .mm-tabs__count {
  color: var(--tone, var(--mm-muted));
}

.mm-tabs__panel {
  margin-top: 1rem;
}

.mm-tabs__panel:focus-visible {
  outline-offset: 6px;
}

/* On narrow screens a long tab list scrolls sideways instead of overflowing the page. */
@media (max-width: 640px) {
  .mm-tabs__list {
    max-width: 100%;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .mm-tabs__trigger {
    flex: none;
    white-space: nowrap;
  }

  /* The scroll container would clip an outset ring. */
  .mm-tabs__trigger:focus-visible {
    outline-offset: -2px;
  }
}
</style>
