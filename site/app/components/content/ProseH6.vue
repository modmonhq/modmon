<script setup lang="ts">
defineProps<{ id?: string }>();
</script>

<template>
  <DocsHeading :level="6" :id="id"><slot /></DocsHeading>
</template>
