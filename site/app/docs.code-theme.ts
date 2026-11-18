// Shiki theme for documentation code blocks. Every colour is a CSS variable, so the palette
// lives with the rest of the prose styles (components/docs/DocsProse.vue) and can change
// without re-parsing the Markdown.
const v = (name: string) => `var(--mm-code-${name})`;

export default {
  name: "mm",
  type: "dark" as const,
  colors: {
    "editor.foreground": v("fg"),
    "editor.background": v("bg"),
  },
  tokenColors: [
    {
      scope: ["comment", "punctuation.definition.comment", "string.comment"],
      settings: { foreground: v("comment") },
    },
    {
      scope: ["string", "string.quoted", "punctuation.definition.string", "markup.inline.raw"],
      settings: { foreground: v("string") },
    },
    {
      scope: ["constant.numeric", "constant.language", "constant.character", "support.constant"],
      settings: { foreground: v("constant") },
    },
    {
      scope: ["keyword", "storage", "storage.type", "storage.modifier", "keyword.control"],
      settings: { foreground: v("keyword") },
    },
    {
      scope: [
        "entity.name.tag",
        "support.type.property-name",
        "meta.object-literal.key",
        "variable.other.env",
        "variable.other.assignment",
        "keyword.other.definition.ini",
      ],
      settings: { foreground: v("key") },
    },
    {
      scope: ["entity.name.function", "entity.name.command", "support.function"],
      settings: { foreground: v("function") },
    },
    {
      scope: ["variable", "punctuation.definition.variable", "variable.parameter"],
      settings: { foreground: v("variable") },
    },
    {
      scope: ["constant.other.option", "entity.other.attribute-name"],
      settings: { foreground: v("option") },
    },
    {
      scope: ["punctuation", "keyword.operator", "meta.brace", "punctuation.separator"],
      settings: { foreground: v("punctuation") },
    },
  ],
};
