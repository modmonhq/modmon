<script setup lang="ts">
import MmButton from "@mm/ui/components/MmButton.vue";
import MmCodeText from "@mm/ui/components/MmCodeText.vue";
import MmNotice from "@mm/ui/components/MmNotice.vue";
import MmSpinner from "@mm/ui/components/MmSpinner.vue";
import MmWordmark from "@mm/ui/components/MmWordmark.vue";

withDefaults(defineProps<{ state: "loading" | "error"; detail?: string }>(), { detail: "" });
defineEmits<{ retry: [] }>();
</script>

<template>
  <main id="main" class="state" aria-live="polite">
    <section class="state__panel">
      <MmWordmark class="state__mark" />
      <template v-if="state === 'loading'">
        <h1 class="state__title">Connecting</h1>
        <p class="state__line"><MmSpinner /><span>Waiting for the local mm API</span></p>
      </template>
      <template v-else>
        <h1 class="state__title">Dashboard unreachable</h1>
        <MmNotice class="state__detail" tone="bad"><MmCodeText :text="detail" /></MmNotice>
        <MmButton variant="primary" @click="$emit('retry')">Try again</MmButton>
      </template>
    </section>
  </main>
</template>
