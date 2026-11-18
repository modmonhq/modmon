<script setup lang="ts">
const props = withDefaults(defineProps<{ href?: string; target?: string }>(), {
  href: "",
  target: undefined,
});

const { site } = useAppConfig();
const source = inject(docsSourceKey, "docs/index.md");

// Files GitHub shows as a page although their names have no extension, and the Markdown
// files at the top of a directory. Nuxt Content lowercases relative `.md` links and drops
// the extension before this component sees them, so these names are restored here.
const PLAIN_FILES = /(^|\/)(LICENSE|Makefile|Dockerfile[^/]*)$/;
const UPPERCASE_DOCS = /(^|\/)(readme|contributing|security|changelog|code_of_conduct|agents)$/;

/**
 * The Markdown links to other files by relative path so the same text works on GitHub.
 * Here a link to another page of the docs becomes its route, a link to any other file in
 * the repository goes to GitHub, and everything else is left alone.
 */
const link = computed<{ kind: "route" | "plain" | "external"; href: string }>(() => {
  const href = props.href;
  if (/^[a-z][a-z\d+.-]*:/i.test(href) || href.startsWith("//")) return { kind: "external", href };
  if (href.startsWith("#")) return { kind: "plain", href };
  if (href.startsWith("/")) {
    // Root-relative links to files (the JSON schemas in public/) are served as they are.
    const isFile = /\.[a-z\d]+$/i.test(href.split(/[?#]/, 1)[0]!);
    return { kind: isFile ? "plain" : "route", href };
  }

  const url = new URL(href, `https://repository.invalid/${source}`);
  const file = decodeURIComponent(url.pathname).replace(/^\/+|\/+$/g, "");
  const page = file.match(/^docs(?:\/(.+?))?(?:\.md)?$/);
  if (page && !/\.[a-z\d]+$/i.test(page[1] ?? "")) {
    const slug = (page[1] ?? "").replace(/(^|\/)index$/, "");
    return { kind: "route", href: `/docs${slug ? `/${slug}` : ""}${url.hash}` };
  }

  let path = file;
  let view = /\.[a-z\d]+$/i.test(file) || PLAIN_FILES.test(file) ? "blob" : "tree";
  if (UPPERCASE_DOCS.test(file)) {
    path = file.replace(/[^/]+$/, (name) => `${name.toUpperCase()}.md`);
    view = "blob";
  }
  return { kind: "external", href: `${site.repo}/${view}/master/${path}${url.hash}` };
});
</script>

<template>
  <NuxtLink v-if="link.kind === 'route'" :to="link.href" :target="target"><slot /></NuxtLink>
  <a v-else-if="link.kind === 'plain'" :href="link.href" :target="target"><slot /></a>
  <a v-else :href="link.href" :target="target" rel="noopener"><slot /></a>
</template>
