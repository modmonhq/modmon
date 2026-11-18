<script setup lang="ts">
// The page title is the only h1. A top-level heading inside the Markdown body (the generated
// CLI reference has one) is rendered a level down.
defineProps<{ id?: string }>();
</script>

<template>
  <DocsHeading :level="2" :id="id"><slot /></DocsHeading>
</template>
