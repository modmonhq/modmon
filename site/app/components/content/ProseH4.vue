<script setup lang="ts">
defineProps<{ id?: string }>();
</script>

<template>
  <DocsHeading :level="4" :id="id"><slot /></DocsHeading>
</template>
