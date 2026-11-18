import type { Ref } from "vue";
import type { DocsTocLink } from "~/composables/useDocs";

/**
 * The id of the heading the reader is currently under: the last one that has scrolled past
 * the top of the reading area. Headings are looked up by id on every pass, so the result
 * stays right while pages swap underneath a persistent layout.
 */
export function useDocsScrollSpy(links: Ref<DocsTocLink[]>) {
  const active = ref<string>();
  const ids = computed(() =>
    links.value.flatMap((link) => [link.id, ...(link.children ?? []).map((child) => child.id)]),
  );
  let frame = 0;

  function measure() {
    const line = (Number.parseFloat(getComputedStyle(document.documentElement).scrollPaddingTop) || 0) + 24;
    let current: string | undefined;
    for (const id of ids.value) {
      const heading = document.getElementById(id);
      if (!heading) continue;
      if (heading.getBoundingClientRect().top > line) break;
      current = id;
    }
    // At the very bottom the last sections may be too short to ever reach the line.
    const atEnd = window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 2;
    if (atEnd && window.scrollY > 0 && ids.value.length) current = ids.value.at(-1);
    active.value = current;
  }

  function schedule() {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(measure);
  }

  const nuxtApp = useNuxtApp();
  const stop = [nuxtApp.hook("page:finish", schedule), nuxtApp.hook("page:transition:finish", schedule)];

  onMounted(() => {
    schedule();
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule, { passive: true });
  });
  onBeforeUnmount(() => {
    window.removeEventListener("scroll", schedule);
    window.removeEventListener("resize", schedule);
    cancelAnimationFrame(frame);
    stop.forEach((off) => off());
  });
  watch(ids, () => import.meta.client && nextTick(schedule));

  return active;
}
