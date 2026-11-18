<script setup lang="ts">
const wrap = ref<HTMLElement>();
const scrollable = useDocsOverflow(wrap);
</script>

<template>
  <div
    ref="wrap"
    class="docs-table"
    :class="{ 'is-scrollable': scrollable }"
    :tabindex="scrollable ? 0 : undefined"
    :role="scrollable ? 'region' : undefined"
    :aria-label="scrollable ? 'Table, scrolls sideways' : undefined"
  >
    <table>
      <slot />
    </table>
  </div>
</template>
