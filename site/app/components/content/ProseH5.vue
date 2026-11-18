<script setup lang="ts">
defineProps<{ id?: string }>();
</script>

<template>
  <DocsHeading :level="5" :id="id"><slot /></DocsHeading>
</template>
