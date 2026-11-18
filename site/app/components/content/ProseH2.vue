<script setup lang="ts">
defineProps<{ id?: string }>();
</script>

<template>
  <DocsHeading :level="2" :id="id"><slot /></DocsHeading>
</template>
