import type { InjectionKey, MaybeRefOrGetter } from "vue";
import { docsNav, docsRoute } from "~/docs.nav";

export interface DocsNavItem {
  path: string;
  title: string;
  group: string;
}

export interface DocsNavSection {
  title: string;
  items: DocsNavItem[];
}

/** An entry in a page's outline, as Nuxt Content generates it from h2 and h3 headings. */
export interface DocsTocLink {
  id: string;
  depth: number;
  text: string;
  children?: DocsTocLink[];
}

/** Repository path of the Markdown file being rendered, such as `docs/start/install.md`. */
export const docsSourceKey: InjectionKey<string> = Symbol("docs-source");

/** A route path as the content collection stores it: no trailing slash. */
export function docsPath(path: string): string {
  return path.replace(/\/+$/, "") || "/";
}

/**
 * The page behind a route. The layout and the page both call this; they share one request
 * because the key is the same.
 */
export function useDocsPage(path: MaybeRefOrGetter<string>) {
  return useAsyncData(
    () => `docs:${toValue(path)}`,
    () => queryCollection("docs").path(toValue(path)).first(),
  );
}

/** The sidebar: groups from docs.nav.ts, labelled with each page's title. */
export async function useDocsNav() {
  const { data } = await useAsyncData("docs-nav", () =>
    queryCollection("docs").select("path", "title").all(),
  );

  const sections = computed<DocsNavSection[]>(() => {
    const titles = new Map((data.value ?? []).map((page) => [page.path, page.title]));
    const listed = new Set<string>();
    const groups = docsNav.map((group) => ({
      title: group.title,
      items: group.pages.flatMap((page) => {
        const path = docsRoute(page);
        const title = titles.get(path);
        listed.add(path);
        return title ? [{ path, title, group: group.title }] : [];
      }),
    }));
    // A page nobody added to docs.nav.ts is still reachable.
    const rest = [...titles]
      .filter(([path]) => !listed.has(path))
      .map(([path, title]) => ({ path, title, group: "More" }));
    if (rest.length) groups.push({ title: "More", items: rest });
    return groups.filter((group) => group.items.length);
  });

  const pages = computed(() => sections.value.flatMap((section) => section.items));

  return { sections, pages };
}

/**
 * Scroll an element into view inside its nearest scrolling ancestor, without ever moving
 * the page itself (which `scrollIntoView` would).
 */
export function revealInScroller(element: HTMLElement, smooth: boolean, margin = 56) {
  let scroller = element.parentElement;
  while (scroller && !/auto|scroll/.test(getComputedStyle(scroller).overflowY)) {
    scroller = scroller.parentElement;
  }
  if (!scroller || scroller === document.documentElement || scroller === document.body) return;
  const box = scroller.getBoundingClientRect();
  const item = element.getBoundingClientRect();
  if (item.top >= box.top + margin && item.bottom <= box.bottom - margin) return;
  const top = scroller.scrollTop + item.top - box.top - box.height / 2 + item.height / 2;
  const calm = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  scroller.scrollTo({ top, behavior: smooth && !calm ? "smooth" : "instant" });
}
