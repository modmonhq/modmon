<script setup lang="ts">
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from "reka-ui";
import type { TranscriptLine } from "../types";

/**
 * What a terminal shows, set as text: commands with their prompt, the output they print,
 * and configuration. Lines keep their columns; a block narrower than its longest line
 * scrolls sideways as one, with a hairline scrollbar.
 *
 * Styling hooks: `--mm-transcript-pad` (inner padding) and `--mm-transcript-size` (font size).
 */
defineProps<{
  /** A plain string is a line of output. */
  lines: readonly (TranscriptLine | string)[];
  /** What the block shows, for assistive technology. */
  label: string;
}>();

interface Part {
  text: string;
  key?: boolean;
}

function normalize(line: TranscriptLine | string): TranscriptLine {
  return typeof line === "string" ? { text: line } : line;
}

/** A YAML comment inside configuration reads as a comment. */
function kindOf(line: TranscriptLine): string {
  if (line.kind === "config" && line.text.trimStart().startsWith("#")) return "comment";
  return line.kind ?? "output";
}

/** Splits a configuration line so its keys can be colored; other lines are one part. */
function parts(line: TranscriptLine): Part[] {
  if (kindOf(line) !== "config") return [{ text: line.text }];
  const found: Part[] = [];
  let at = 0;
  // A key ends in a colon followed by a space or the end of the line, so `host:port` values stay whole.
  for (const match of line.text.matchAll(/[A-Za-z_"*][\w."*-]*:(?=\s|$)/g)) {
    if (match.index > at) found.push({ text: line.text.slice(at, match.index) });
    found.push({ text: match[0], key: true });
    at = match.index + match[0].length;
  }
  if (at < line.text.length) found.push({ text: line.text.slice(at) });
  return found;
}
</script>

<template>
  <ScrollAreaRoot class="mm-transcript" type="auto" :scroll-hide-delay="400">
    <ScrollAreaViewport class="mm-transcript__viewport" role="group" :aria-label="label" tabindex="0">
      <div class="mm-transcript__lines">
        <div
          v-for="(line, index) in lines.map(normalize)"
          :key="index"
          class="mm-transcript__line"
          :data-kind="kindOf(line)"
          :data-tone="line.tone"
        >
          <span v-if="kindOf(line) === 'command'" class="mm-transcript__prompt" aria-hidden="true">$ </span
          ><template v-for="(part, at) in parts(line)" :key="at"
            ><span v-if="part.key" class="mm-transcript__key">{{ part.text }}</span
            ><template v-else>{{ part.text }}</template></template
          >
        </div>
      </div>
    </ScrollAreaViewport>
    <ScrollAreaScrollbar class="mm-transcript__bar" orientation="horizontal">
      <ScrollAreaThumb class="mm-transcript__thumb" />
    </ScrollAreaScrollbar>
  </ScrollAreaRoot>
</template>

<style>
.mm-transcript {
  --mm-transcript-pad: 1rem;
  --mm-transcript-size: var(--mm-text-sm);

  position: relative;
  min-width: 0;
  overflow: hidden;
  font-family: var(--mm-font-mono);
  font-size: var(--mm-transcript-size);
  font-variant-ligatures: none;
  line-height: 1.65;
  color: var(--mm-ink-soft);
}

.mm-transcript__viewport {
  width: 100%;
  overscroll-behavior-x: contain;
}

.mm-transcript__viewport:focus-visible {
  outline-offset: -2px;
}

/* The lines share one width, the longest line's, so padding holds at the end of the scroll. */
.mm-transcript__lines {
  width: max-content;
  min-width: 100%;
  padding: var(--mm-transcript-pad);
  white-space: pre;
}

.mm-transcript__line {
  min-height: 1.65em;
}

.mm-transcript__line[data-kind="command"] {
  font-weight: 500;
  color: var(--mm-ink);
}

.mm-transcript__line[data-kind="comment"] {
  color: var(--mm-faint);
}

.mm-transcript__line[data-kind="config"] {
  color: var(--mm-ink);
}

.mm-transcript__line[data-tone] {
  color: var(--tone);
}

.mm-transcript__prompt,
.mm-transcript__key {
  color: var(--mm-accent);
}

.mm-transcript__prompt {
  user-select: none;
}

.mm-transcript__bar {
  display: flex;
  flex-direction: column;
  height: 9px;
  padding: 3px;
  touch-action: none;
  user-select: none;
}

.mm-transcript__thumb {
  position: relative;
  flex: 1;
  background: rgb(232 246 214 / 18%);
  border-radius: 999px;
  transition: background-color 0.2s var(--mm-ease);
}

.mm-transcript__thumb:hover {
  background: rgb(232 246 214 / 34%);
}
</style>
