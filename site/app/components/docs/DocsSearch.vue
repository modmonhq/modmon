<script setup lang="ts">
import type MiniSearch from "minisearch";
import { ListboxContent, ListboxFilter, ListboxItem, ListboxRoot } from "reka-ui";
import type { Hint } from "@mm/ui/types";
import type { DocsNavItem } from "~/composables/useDocs";
import type { DocsSearchSection } from "~~/server/routes/docs-search.json";

/**
 * The search palette. This component is loaded the first time search is opened, and it
 * fetches the section list and builds the index then, so pages do not pay for search
 * until someone uses it.
 */
const props = defineProps<{ pages: DocsNavItem[] }>();
const open = defineModel<boolean>("open", { default: false });

interface Segment {
  text: string;
  hit: boolean;
}

interface Result {
  id: string;
  page: string;
  group?: string;
  heading?: Segment[];
  excerpt: Segment[];
}

const LIMIT = 24;
const hints: Hint[] = [
  ["↑↓", "select"],
  ["enter", "open"],
  ["esc", "close"],
];

const query = ref("");
const state = ref<"loading" | "ready" | "failed">("loading");
const listbox = ref<{ highlightFirstItem: () => void }>();
const results = ref<{ $el: HTMLElement }>();
let index: MiniSearch<DocsSearchSection> | undefined;
const sections = new Map<string, DocsSearchSection>();

async function load() {
  state.value = "loading";
  try {
    const [{ default: MiniSearch }, data] = await Promise.all([
      import("minisearch"),
      $fetch<DocsSearchSection[]>("/docs-search.json"),
    ]);
    const engine = new MiniSearch<DocsSearchSection>({
      fields: ["title", "trail", "content"],
      extractField: (section, field) =>
        field === "trail" ? section.titles.join(" ") : (section[field as "id" | "title" | "content"] ?? ""),
      searchOptions: {
        prefix: true,
        fuzzy: 0.15,
        boost: { title: 5, trail: 2 },
        // A page as a whole is a better first answer than one of its sections.
        boostDocument: (id) => (String(id).includes("#") ? 1 : 2),
      },
    });
    engine.addAll(data);
    for (const section of data) sections.set(section.id, section);
    index = engine;
    state.value = "ready";
  } catch {
    state.value = "failed";
  }
}

const byPath = computed(() => new Map(props.pages.map((page) => [page.path, page])));

function tokens(text: string): string[] {
  return text
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);
}

function mark(text: string, terms: string[]): Segment[] {
  if (!terms.length) return [{ text, hit: false }];
  const escaped = terms.map((term) => term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const pattern = new RegExp(`(${escaped.join("|")})`, "giu");
  return text
    .split(pattern)
    .map((part, position) => ({ text: part, hit: position % 2 === 1 }))
    .filter((part) => part.text);
}

function excerpt(text: string, terms: string[]): Segment[] {
  const width = 160;
  const lower = text.toLowerCase();
  const positions = terms.map((term) => lower.indexOf(term)).filter((position) => position >= 0);
  const first = positions.length ? Math.min(...positions) : 0;
  const start = Math.max(0, first - 44);
  let slice = text.slice(start, start + width);
  if (start > 0) slice = `…${slice.replace(/^\S*\s/, "")}`;
  if (start + width < text.length) slice = `${slice.replace(/\s\S*$/, "")}…`;
  return mark(slice, terms);
}

const found = computed<Result[]>(() => {
  const text = query.value.trim();
  if (state.value !== "ready" || !index || !text) return [];
  let matches = index.search(text, { combineWith: "AND" });
  if (!matches.length) matches = index.search(text);
  const typed = tokens(text);
  return matches.slice(0, LIMIT).flatMap((match) => {
    const section = sections.get(match.id);
    if (!section) return [];
    // Longest first, so "deploy" does not stop "deployment" from being marked whole.
    const terms = [...new Set([...typed, ...match.terms])].sort((a, b) => b.length - a.length);
    const path = section.id.split("#", 1)[0]!;
    const isPage = !section.titles.length;
    return [
      {
        id: section.id,
        // Not titles[0]: a page whose body has its own top-level heading would show that.
        page: byPath.value.get(path)?.title ?? (isPage ? section.title : section.titles[0]!),
        group: byPath.value.get(path)?.group,
        heading: isPage ? undefined : mark(section.title, terms),
        excerpt: excerpt(section.content, terms),
      },
    ];
  });
});

// Before anything is typed the palette is a quick way to the first pages of the docs.
const suggested = computed<Result[]>(() =>
  props.pages
    .slice(0, 8)
    .map((page) => ({ id: page.path, page: page.title, group: page.group, excerpt: [] })),
);

const searching = computed(() => query.value.trim().length > 0);
const items = computed(() => (searching.value ? found.value : suggested.value));
const summary = computed(() => {
  if (state.value === "loading") return "indexing";
  if (state.value === "failed") return "unavailable";
  if (!searching.value) return `${sections.size} sections`;
  const count = found.value.length;
  return `${count === LIMIT ? `${LIMIT}+` : count} ${count === 1 ? "match" : "matches"}`;
});

function highlightFirst() {
  nextTick(() => {
    listbox.value?.highlightFirstItem();
    if (results.value) results.value.$el.scrollTop = 0;
  });
}

async function go(id: string, event: CustomEvent<{ originalEvent: Event }>) {
  const original = event.detail?.originalEvent;
  if (original instanceof MouseEvent && (original.metaKey || original.ctrlKey)) {
    window.open(id, "_blank", "noopener");
    return;
  }
  open.value = false;
  await navigateTo(id);
}

onMounted(load);
watch([items, open], highlightFirst, { flush: "post" });
watch(open, (isOpen) => {
  if (!isOpen) query.value = "";
  else if (state.value === "failed") load();
});
</script>

<template>
  <MmDialog
    v-model:open="open"
    title="Search docs"
    description="Type to search every page of the mm documentation. Use the arrow keys to choose a result and Enter to open it."
    placement="top"
    width="42rem"
  >
    <template #meta>
      <span class="docs-search__summary" aria-live="polite">{{ summary }}</span>
    </template>
    <ListboxRoot ref="listbox" class="docs-search__box" highlight-on-hover>
      <div class="docs-search__field">
        <span class="docs-search__prompt" aria-hidden="true">/</span>
        <ListboxFilter
          v-model="query"
          class="docs-search__input"
          auto-focus
          type="search"
          placeholder="Search commands, settings, guides…"
          aria-label="Search the documentation"
          autocomplete="off"
          autocapitalize="off"
          spellcheck="false"
          enterkeyhint="go"
        />
        <MmSpinner v-if="state === 'loading'" />
      </div>

      <ListboxContent ref="results" class="docs-search__results">
        <p v-if="state === 'failed'" class="docs-search__note">
          <MmBadge tone="bad">The search index did not load.</MmBadge>
          <button type="button" class="docs-search__retry" @click="load">Try again</button>
        </p>
        <p v-else-if="searching && state === 'ready' && !found.length" class="docs-search__note">
          <MmBadge tone="idle">Nothing matches “{{ query.trim() }}”.</MmBadge>
          <span>Try a command name, a setting, or fewer words.</span>
        </p>
        <template v-else>
          <p v-if="!searching" class="docs-search__label">Start here</p>
          <ListboxItem
            v-for="item in items"
            :key="item.id"
            :value="item.id"
            class="docs-search__item"
            @select="go(item.id, $event)"
          >
            <span class="docs-search__title">
              <span class="docs-search__page">{{ item.page }}</span>
              <template v-if="item.heading">
                <span class="docs-search__sep" aria-hidden="true">›</span>
                <span class="docs-search__heading">
                  <template v-for="(part, position) in item.heading" :key="position">
                    <mark v-if="part.hit">{{ part.text }}</mark>
                    <template v-else>{{ part.text }}</template>
                  </template>
                </span>
              </template>
              <span v-if="item.group" class="docs-search__group">{{ item.group }}</span>
            </span>
            <span v-if="item.excerpt.length" class="docs-search__excerpt">
              <template v-for="(part, position) in item.excerpt" :key="position">
                <mark v-if="part.hit">{{ part.text }}</mark>
                <template v-else>{{ part.text }}</template>
              </template>
            </span>
          </ListboxItem>
        </template>
      </ListboxContent>

      <div class="docs-search__foot">
        <MmHints :items="hints" />
      </div>
    </ListboxRoot>
  </MmDialog>
</template>

<style>
/* Not scoped: the dialog is teleported to the body. */
/* Rows run edge to edge inside the card's 1px frame, so the box clips them to the frame's
   inner corners. */
.docs-search__box {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  margin: 0 1px 1px;
  overflow: hidden;
  border-radius: 0 0 calc(var(--mm-card-radius) - 1px) calc(var(--mm-card-radius) - 1px);
}

.docs-search__summary {
  color: var(--mm-muted);
}

.docs-search__field {
  display: flex;
  flex: none;
  gap: 0.75rem;
  align-items: center;
  padding: 0.55rem 1.1rem 0.85rem;
  border-bottom: 1px solid var(--mm-line);
}

.docs-search__prompt {
  flex: none;
  font-weight: 700;
  color: var(--mm-accent);
}

.docs-search__input {
  flex: 1;
  min-width: 0;
  height: 2rem;
  padding: 0;
  font-family: var(--mm-font-mono);
  /* 16px keeps iOS from zooming the page when the field takes focus. */
  font-size: 1rem;
  color: var(--mm-ink);
  caret-color: var(--mm-accent);
  background: none;
  border: 0;
  outline: none;
  appearance: none;
}

.docs-search__input::placeholder {
  color: var(--mm-faint);
}

.docs-search__input::-webkit-search-cancel-button {
  display: none;
}

.docs-search__results {
  flex: 1;
  min-height: 0;
  padding: 0.5rem 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  scroll-padding-block: 0.5rem;
  scrollbar-width: thin;
}

.docs-search__label {
  margin: 0;
  padding: 0.35rem 1.1rem 0.4rem;
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.14em;
  color: var(--mm-muted);
  text-transform: uppercase;
}

.docs-search__item {
  position: relative;
  display: grid;
  gap: 0.2rem;
  padding: 0.55rem 1.1rem;
  cursor: pointer;
  outline: none;
}

.docs-search__item[data-highlighted] {
  background: var(--mm-selection);
}

.docs-search__item[data-highlighted]::before {
  position: absolute;
  inset: 0 auto 0 0;
  width: 3px;
  content: "";
  background: var(--mm-accent);
}

.docs-search__title {
  display: flex;
  gap: 0.6em;
  align-items: baseline;
  min-width: 0;
  color: var(--mm-ink-soft);
}

.docs-search__page {
  flex: none;
  max-width: 60%;
  overflow: hidden;
  font-weight: 700;
  color: var(--mm-ink);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.docs-search__sep {
  flex: none;
  color: var(--mm-faint);
}

.docs-search__heading {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.docs-search__group {
  flex: none;
  margin-left: auto;
  padding-left: 1rem;
  font-size: 0.6875rem;
  letter-spacing: 0.1em;
  color: var(--mm-faint);
  text-transform: uppercase;
  white-space: nowrap;
}

.docs-search__item[data-highlighted] .docs-search__group {
  color: var(--mm-muted);
}

.docs-search__excerpt {
  display: -webkit-box;
  overflow: hidden;
  font-family: var(--mm-font-sans);
  font-size: 0.8125rem;
  line-height: 1.5;
  color: var(--mm-muted);
  overflow-wrap: anywhere;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  line-clamp: 2;
}

.docs-search__box mark {
  font-weight: 600;
  color: var(--mm-accent);
  background: none;
}

.docs-search__excerpt mark {
  color: var(--mm-ink);
  text-decoration: underline;
  text-decoration-color: var(--mm-accent);
  text-decoration-thickness: 1px;
  text-underline-offset: 0.2em;
}

.docs-search__note {
  display: grid;
  gap: 0.5rem;
  justify-items: start;
  margin: 0;
  padding: 1.25rem 1.1rem 1.5rem;
  color: var(--mm-muted);
}

.docs-search__note .mm-badge {
  white-space: normal;
}

.docs-search__retry {
  padding: 0;
  font: inherit;
  color: var(--mm-ink);
  text-decoration: underline;
  text-decoration-color: var(--mm-accent);
  text-underline-offset: 0.22em;
  cursor: pointer;
  background: none;
  border: 0;
}

.docs-search__foot {
  display: flex;
  flex: none;
  justify-content: space-between;
  padding: 0.6rem 1.1rem 0.75rem;
  font-size: var(--mm-text-xs);
  border-top: 1px solid var(--mm-line);
}

@media (max-width: 560px) {
  .docs-search__group {
    display: none;
  }

  .docs-search__foot {
    display: none;
  }
}
</style>
