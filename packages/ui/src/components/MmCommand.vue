<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";

/** One shell command with a prompt and a copy button. */
const props = defineProps<{ command: string; comment?: string }>();

const copied = ref(false);
let timer: ReturnType<typeof setTimeout> | undefined;

async function copy() {
  try {
    await navigator.clipboard.writeText(props.command);
    copied.value = true;
    clearTimeout(timer);
    timer = setTimeout(() => (copied.value = false), 1600);
  } catch {
    // Clipboard access can be refused; the command stays selectable.
  }
}

onBeforeUnmount(() => clearTimeout(timer));
</script>

<template>
  <div class="mm-command">
    <span class="mm-command__prompt" aria-hidden="true">$</span>
    <code class="mm-command__text">{{ command }}</code>
    <span v-if="comment" class="mm-command__comment"># {{ comment }}</span>
    <button type="button" class="mm-command__copy" :class="{ 'is-copied': copied }" @click="copy">
      <svg v-if="!copied" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
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
      <svg v-else viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
        <path
          d="m3.5 8.5 3 3 6-7"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
      <span class="mm-sr-only" aria-live="polite">{{ copied ? "Copied" : `Copy command: ${command}` }}</span>
    </button>
  </div>
</template>

<style>
/* The copy button sits one gap inside every edge, so its radius is the outer one less the gap. */
.mm-command {
  --mm-command-height: 2.6rem;
  --mm-command-radius: 0px;
  --mm-command-gap: 0.375rem;

  display: flex;
  gap: 0.75em;
  align-items: center;
  min-width: 0;
  height: var(--mm-command-height);
  padding: 0 var(--mm-command-gap) 0 0.95rem;
  border-radius: var(--mm-command-radius);
  font-family: var(--mm-font-mono);
  font-size: var(--mm-text-sm);
}

.mm-command__prompt {
  flex: none;
  color: var(--mm-accent);
  user-select: none;
}

/* Long commands scroll sideways; the fade shows there is more. */
.mm-command__text {
  flex: 1;
  min-width: 0;
  padding-right: 1.25rem;
  overflow-x: auto;
  color: var(--mm-ink);
  white-space: nowrap;
  scrollbar-width: none;
  mask-image: linear-gradient(90deg, #000 calc(100% - 1.25rem), transparent);
}

.mm-command__comment {
  flex: none;
  color: var(--mm-faint);
  white-space: nowrap;
  user-select: none;
}

.mm-command__copy {
  display: grid;
  flex: none;
  place-items: center;
  width: calc(var(--mm-command-height) - 2 * var(--mm-command-gap));
  height: calc(var(--mm-command-height) - 2 * var(--mm-command-gap));
  color: var(--mm-muted);
  cursor: pointer;
  background: none;
  border: 0;
  border-radius: max(4px, calc(var(--mm-command-radius) - var(--mm-command-gap)));
  transition:
    color 0.2s var(--mm-ease),
    background-color 0.2s var(--mm-ease);
}

.mm-command__copy:hover {
  color: var(--mm-ink);
  background: rgb(255 255 255 / 7%);
}

.mm-command__copy.is-copied {
  color: var(--mm-accent);
}

@media (max-width: 640px) {
  .mm-command__comment {
    display: none;
  }
}
</style>
