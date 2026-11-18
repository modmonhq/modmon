<script setup lang="ts">
/** A list of labelled values in two aligned columns. Fill it with `MmKvItem`. */
</script>

<template>
  <dl class="mm-kv"><slot /></dl>
</template>

<style>
.mm-kv {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: 0.3em 1.75em;
  margin: 0;
}

.mm-kv__item {
  display: contents;
}

.mm-kv__label {
  color: var(--mm-muted);
}

.mm-kv__value {
  min-width: 0;
  margin: 0;
  color: var(--mm-ink);
  overflow-wrap: anywhere;
}
</style>
