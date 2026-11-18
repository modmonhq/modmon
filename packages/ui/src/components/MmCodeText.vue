<script setup lang="ts">
import { computed } from "vue";

/** Plain text whose `backtick` spans are set as inline code, the way CLI messages name commands. */
const props = defineProps<{ text: string }>();

const parts = computed(() => {
  const pieces = props.text.split("`");
  return pieces.map((value, index) => {
    const code = index % 2 === 1;
    // An unmatched backtick stays in the text as written.
    return code && index === pieces.length - 1 ? { value: `\`${value}`, code: false } : { value, code };
  });
});
</script>

<template>
  <span class="mm-code-text">
    <template v-for="(part, index) in parts" :key="index">
      <code v-if="part.code">{{ part.value }}</code>
      <template v-else>{{ part.value }}</template>
    </template>
  </span>
</template>

<style>
.mm-code-text code {
  font-size: 0.94em;
  color: var(--mm-ink);
}
</style>
