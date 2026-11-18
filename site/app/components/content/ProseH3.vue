<script setup lang="ts">
defineProps<{ id?: string }>();
</script>

<template>
  <DocsHeading :level="3" :id="id"><slot /></DocsHeading>
</template>
