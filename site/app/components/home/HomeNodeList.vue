<script setup lang="ts">
import type { TranscriptLine } from "@mm/ui/types";

/*
 * The smallest cluster target `mm config validate` accepts: a name and a host for each node.
 * Mirrors docs/reference/configuration.md, "Cluster targets". The terminal next to it runs
 * the same three nodes.
 */
const lines: TranscriptLine[] = [
  { kind: "config", text: "targets:" },
  { kind: "config", text: "  production:" },
  { kind: "config", text: "    nodes:" },
  { kind: "config", text: "      - {name: app-1, host: 203.0.113.11}" },
  { kind: "config", text: "      - {name: app-2, host: 203.0.113.12}" },
  { kind: "config", text: "      - {name: db-1,  host: 203.0.113.13}" },
];
</script>

<template>
  <MmWindow class="nodelist">
    <template #title>.config/mm.yaml</template>
    <MmTranscript :lines="lines" label="A cluster target in .config/mm.yaml" />
    <p class="nodelist__note">
      <MmTone tone="ok" />
      <span
        >A complete cluster target. The mesh, the network ranges, and the storage node have defaults.</span
      >
    </p>
  </MmWindow>
</template>

<style scoped>
.nodelist {
  --mm-window-bg: #080a09;

  box-shadow:
    0 0 0 1px rgb(232 246 214 / 15%),
    inset 0 1px 0 rgb(255 255 255 / 8%),
    0 40px 80px -50px rgb(0 0 0 / 90%),
    0 24px 60px -36px rgb(var(--mm-accent-rgb) / 22%);
}

.nodelist :deep(.mm-transcript) {
  --mm-transcript-pad: 1rem 1.15rem;
  --mm-transcript-size: clamp(0.8125rem, 1.02vw, 0.9375rem);
}

.nodelist__note {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 0.65rem;
  align-items: start;
  margin: 0;
  padding: 0.7rem 1.15rem 0.8rem;
  font-size: 0.8438rem;
  line-height: 1.45;
  color: var(--mm-muted);
  border-top: 1px solid var(--mm-line);
}

.nodelist__note .mm-tone {
  margin-top: 0.3em;
  font-size: 0.7rem;
}
</style>
