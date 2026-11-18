<template>
  <div class="docs-prose">
    <slot />
  </div>
</template>

<!-- Global on purpose: these rules style whatever the Markdown renderer produces. -->
<style>
.docs-prose {
  /* Code colours, referenced by the Shiki theme in app/docs.code-theme.ts. */
  --mm-code-fg: #cfd6d1;
  --mm-code-bg: transparent;
  --mm-code-comment: #7f8883;
  --mm-code-string: #bfd69a;
  --mm-code-constant: var(--mm-warn);
  --mm-code-keyword: var(--mm-accent);
  --mm-code-key: var(--mm-info);
  --mm-code-function: var(--mm-ink);
  --mm-code-variable: #a9dcff;
  --mm-code-option: #a5d3f0;
  --mm-code-punctuation: #8d9690;

  font-size: 1rem;
  line-height: 1.7;
  color: var(--mm-ink-soft);
  overflow-wrap: break-word;
}

/* Text keeps to the reading measure; tables and code get the whole column. */
.docs-prose > * {
  max-width: var(--docs-measure, 44rem);
  margin-block: 0 1.25rem;
}

.docs-prose > .docs-code,
.docs-prose > .docs-table {
  max-width: none;
}

.docs-prose > :last-child {
  margin-bottom: 0;
}

.docs-prose p {
  text-wrap: pretty;
}

.docs-prose strong {
  font-weight: 600;
  color: var(--mm-ink);
}

/* Headings */
.docs-prose :is(h2, h3, h4, h5, h6) {
  color: var(--mm-ink);
  text-wrap: balance;
}

.docs-prose h2 {
  margin-block: 3.25rem 1rem;
  font-family: var(--mm-font-display);
  font-size: 1.75rem;
  font-weight: 600;
  line-height: 1.15;
  letter-spacing: -0.006em;
}

.docs-prose h3 {
  margin-block: 2.5rem 0.75rem;
  font-family: var(--mm-font-display);
  font-size: 1.3125rem;
  font-weight: 600;
  line-height: 1.25;
}

.docs-prose h4 {
  margin-block: 2rem 0.6rem;
  font-size: 1rem;
  font-weight: 600;
  line-height: 1.4;
}

.docs-prose :is(h5, h6) {
  margin-block: 1.75rem 0.6rem;
  font-family: var(--mm-font-mono);
  font-size: 0.75rem;
  font-weight: 600;
  line-height: 1.5;
  letter-spacing: 0.12em;
  color: var(--mm-ink-soft);
  text-transform: uppercase;
}

.docs-prose > :first-child {
  margin-top: 0;
}

.docs-prose :is(h2, h3, h4, h5, h6) + * {
  margin-top: 0;
}

/* Links: ink text on a lime rule that fills in on hover. */
.docs-prose a:not(.docs-heading__anchor) {
  font-weight: 500;
  color: var(--mm-ink);
  text-decoration: underline;
  text-decoration-color: rgb(var(--mm-accent-rgb) / 55%);
  text-decoration-thickness: 1px;
  text-underline-offset: 0.22em;
  border-radius: 2px;
  transition:
    color 0.15s var(--mm-ease),
    text-decoration-color 0.15s var(--mm-ease);
}

.docs-prose a:not(.docs-heading__anchor):hover {
  color: var(--mm-accent);
  text-decoration-color: var(--mm-accent);
}

.docs-prose a:focus-visible {
  outline-offset: 2px;
}

/* Lists */
.docs-prose :is(ul, ol) {
  padding-left: 1.35rem;
}

.docs-prose li {
  margin-block: 0.4rem;
  padding-left: 0.3rem;
}

.docs-prose li > :is(ul, ol) {
  margin-block: 0.4rem;
}

.docs-prose li > p {
  margin-block: 0 0.6rem;
}

.docs-prose li > :is(.docs-code, .docs-table) {
  margin-block: 0.75rem 1rem;
}

.docs-prose ul > li::marker {
  color: var(--mm-faint);
}

.docs-prose ol > li::marker {
  font-family: var(--mm-font-mono);
  font-size: 0.875em;
  font-weight: 600;
  color: var(--mm-muted);
}

/* Inline code */
.docs-prose :not(pre) > code {
  padding: 0.12em 0.38em;
  font-size: 0.875em;
  font-weight: 450;
  color: var(--mm-ink);
  overflow-wrap: anywhere;
  background: rgb(232 246 214 / 6%);
  border: 1px solid var(--mm-line);
  border-radius: 4px;
}

.docs-prose a > code {
  color: inherit;
}

/* A command or setting used as a heading reads as the heading, not as a chip inside it. */
.docs-prose :is(h2, h3, h4, h5, h6) > code {
  padding: 0;
  font-size: 0.9em;
  font-weight: 600;
  letter-spacing: -0.01em;
  background: none;
  border: 0;
}

/* Code blocks: a terminal card with the language set into its top border. */
.docs-code.mm-card {
  --mm-card-line: var(--mm-line-strong);
  --mm-card-pad: 1rem;
  --mm-card-surface: rgb(4 5 4 / 55%);

  font-family: var(--mm-font-mono);
  font-size: 0.75rem;
}

/* A definite row height: the card's rule is half of it, which a min-height cannot give. */
.docs-code .mm-card__top {
  height: var(--mm-card-row);
}

.docs-code .mm-card__title {
  font-weight: 500;
  letter-spacing: 0.04em;
  color: var(--mm-muted);
}

.docs-code .mm-card__meta {
  display: flex;
  padding: 0 0.25em;
}

.docs-code .mm-card__body {
  padding: 0;
}

/* Further from the card's corner than its radius, so concentric means square. */
.docs-code__copy,
.docs-code__copy:focus-visible {
  border-radius: max(0px, calc(var(--mm-card-radius) - var(--mm-card-pad)));
}

.docs-code__copy {
  display: inline-flex;
  gap: 0.45em;
  align-items: center;
  height: var(--mm-card-row);
  padding: 0 0.5em;
  font: inherit;
  line-height: 1;
  color: var(--mm-muted);
  cursor: pointer;
  background: none;
  border: 0;
  transition: color 0.15s var(--mm-ease);
}

.docs-code__copy:hover {
  color: var(--mm-ink);
}

.docs-code__copy:focus-visible {
  outline-offset: -1px;
}

.docs-code.is-copied .docs-code__copy {
  color: var(--mm-accent);
}

/* The block fills the card edge to edge, so it shares the card's bottom corners. */
.docs-code pre,
.docs-code pre:focus-visible {
  border-radius: 0 0 var(--mm-card-radius) var(--mm-card-radius);
}

.docs-code pre {
  margin: 0;
  padding: 0.7rem var(--mm-card-pad) 0.95rem;
  overflow-x: auto;
  font-size: 0.8125rem;
  line-height: 1.7;
  color: var(--mm-code-fg);
  tab-size: 2;
  scrollbar-width: thin;
}

.docs-code pre:focus-visible {
  outline-offset: -2px;
}

.docs-code pre code {
  display: block;
  width: max-content;
  min-width: 100%;
  font-size: inherit;
}

.docs-code pre code .line {
  display: block;
  min-height: 1lh;
}

.docs-prose .shiki span {
  color: var(--shiki-default);
}

/* Tables: the wrapper clips its rows, so cells need no radius of their own. */
.docs-table {
  --docs-table-radius: var(--mm-radius);

  overflow-x: auto;
  border: 1px solid var(--mm-line-strong);
  border-radius: var(--docs-table-radius);
  scrollbar-width: thin;
}

.docs-table:focus-visible {
  outline-offset: 2px;
  border-radius: var(--docs-table-radius);
}

.docs-table table {
  width: 100%;
  font-size: 0.875rem;
  line-height: 1.55;
  border-spacing: 0;
  border-collapse: separate;
}

.docs-table th {
  padding: 0.6rem 1rem;
  font-family: var(--mm-font-mono);
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: var(--mm-muted);
  text-align: left;
  text-transform: uppercase;
  white-space: nowrap;
  background: rgb(232 246 214 / 3.5%);
  border-bottom: 1px solid var(--mm-line-strong);
}

.docs-table td {
  min-width: 9rem;
  padding: 0.7rem 1rem;
  vertical-align: top;
  border-bottom: 1px solid var(--mm-line);
}

.docs-table tr:last-child > td {
  border-bottom: 0;
}

.docs-table td:first-child {
  color: var(--mm-ink);
}

/* Setting names in the first column stay on one line; the table scrolls instead. */
.docs-table td:first-child code {
  overflow-wrap: normal;
  white-space: nowrap;
}

.docs-table tbody tr {
  transition: background-color 0.15s var(--mm-ease);
}

.docs-table tbody tr:hover {
  background: rgb(255 255 255 / 2%);
}

.docs-table :is(th, td)[align="center"] {
  text-align: center;
}

.docs-table :is(th, td)[align="right"] {
  text-align: right;
}

/* Quotes and rules */
.docs-prose blockquote {
  margin-inline: 0;
  padding: 0.15rem 0 0.15rem 1.1rem;
  color: var(--mm-ink-soft);
  border-left: 2px solid var(--mm-info);
}

.docs-prose blockquote > :last-child {
  margin-bottom: 0;
}

.docs-prose hr {
  height: 1px;
  margin-block: 2.75rem;
  background: var(--mm-line-strong);
  border: 0;
}

.docs-prose img {
  max-width: 100%;
  height: auto;
  border: 1px solid var(--mm-line);
  border-radius: var(--mm-radius);
}

.docs-prose kbd {
  padding: 0.1em 0.4em;
  font-size: 0.8125em;
  font-weight: 600;
  color: var(--mm-ink);
  background: rgb(255 255 255 / 6%);
  border: 1px solid var(--mm-line-strong);
  border-bottom-width: 2px;
  border-radius: 5px;
}

@media (max-width: 640px) {
  .docs-prose h2 {
    margin-top: 2.75rem;
    font-size: 1.5rem;
  }

  .docs-prose h3 {
    font-size: 1.1875rem;
  }

  /* Let code and tables run to the screen edge instead of losing width to the gutter. */
  .docs-table td {
    min-width: 8rem;
    padding: 0.6rem 0.8rem;
  }

  .docs-table th {
    padding-inline: 0.8rem;
  }
}
</style>
