<script setup lang="ts">
/** One row of an `MmKv` list: a muted label and its value. */
defineProps<{ label: string }>();
</script>

<template>
  <div class="mm-kv__item">
    <dt class="mm-kv__label">{{ label }}</dt>
    <dd class="mm-kv__value"><slot /></dd>
  </div>
</template>
