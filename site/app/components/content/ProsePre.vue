<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    code?: string;
    language?: string;
    filename?: string;
    highlights?: number[];
    meta?: string;
    class?: string;
  }>(),
  {
    code: "",
    language: undefined,
    filename: undefined,
    highlights: () => [],
    meta: undefined,
    class: undefined,
  },
);

const label = computed(() => props.filename ?? props.language ?? "text");

const pre = ref<HTMLElement>();
const scrollable = useDocsOverflow(pre);

const copied = ref(false);
let timer: ReturnType<typeof setTimeout> | undefined;

async function copy() {
  try {
    await navigator.clipboard.writeText(props.code.replace(/\n$/, ""));
    copied.value = true;
    clearTimeout(timer);
    timer = setTimeout(() => (copied.value = false), 1600);
  } catch {
    // Clipboard access can be refused; the code stays selectable.
  }
}

onBeforeUnmount(() => clearTimeout(timer));
</script>

<template>
  <MmCard as="div" class="docs-code" :class="{ 'is-copied': copied }">
    <template #title>{{ label }}</template>
    <template #meta>
      <button
        type="button"
        class="docs-code__copy"
        :aria-label="copied ? 'Copied' : 'Copy code'"
        @click="copy"
      >
        <svg v-if="!copied" viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
          <rect
            x="5.5"
            y="5.5"
            width="8"
            height="8"
            rx="1.6"
            fill="none"
            stroke="currentColor"
            stroke-width="1.4"
          />
          <path
            d="M10.5 3.2H4.4A1.4 1.4 0 0 0 3 4.6v6.1"
            fill="none"
            stroke="currentColor"
            stroke-width="1.4"
            stroke-linecap="round"
          />
        </svg>
        <svg v-else viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
          <path
            d="m3.5 8.5 3 3 6-7"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        <span aria-live="polite">{{ copied ? "copied" : "copy" }}</span>
      </button>
    </template>
    <pre
      ref="pre"
      :class="props.class"
      :tabindex="scrollable ? 0 : undefined"
      :role="scrollable ? 'region' : undefined"
      :aria-label="scrollable ? `Code: ${label}` : undefined"
    ><slot /></pre>
  </MmCard>
</template>
