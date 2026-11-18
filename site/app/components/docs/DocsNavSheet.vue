<script setup lang="ts">
import type { DocsNavSection } from "~/composables/useDocs";

/** The sidebar as a slide-over, for screens too narrow to keep it on the page. */
defineProps<{ sections: DocsNavSection[]; current: string }>();
defineEmits<{ search: [] }>();

const open = defineModel<boolean>("open", { default: false });
</script>

<template>
  <MmDrawer v-model:open="open" title="Documentation" description="Browse the documentation pages.">
    <div class="docs-sheet">
      <DocsSearchButton @click="$emit('search')" />
      <DocsSidebar :sections="sections" :current="current" />
    </div>
  </MmDrawer>
</template>

<style scoped>
.docs-sheet {
  display: grid;
  gap: 1.5rem;
  padding: 1.1rem 1rem 3rem;
}
</style>
