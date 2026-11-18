import { queryCollectionSearchSections } from "@nuxt/content/server";

export interface DocsSearchSection {
  /** Route of the section: the page path, plus `#heading` for everything below the intro. */
  id: string;
  /** The heading, or the page title for a page's introduction. */
  title: string;
  /** The page title followed by any parent headings. Empty for a page's introduction. */
  titles: string[];
  /** Plain text of the section, code blocks included. */
  content: string;
}

// The search index source: every page of the docs split at its headings. It is prerendered
// to a static file, and the browser builds the index from it the first time search opens.
export default defineEventHandler(async (event): Promise<DocsSearchSection[]> => {
  // Highlighted code blocks carry a <style> element whose text is not content.
  const sections = await queryCollectionSearchSections(event, "docs", { ignoredTags: ["style"] });
  return sections.map(({ id, title, titles, content }) => ({
    id,
    title,
    titles,
    content: content.replace(/\s+/g, " ").trim(),
  }));
});
