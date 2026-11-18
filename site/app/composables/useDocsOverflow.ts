import type { Ref } from "vue";

// One observer for every code block and table on the page.
let observer: ResizeObserver | undefined;
const watched = new WeakMap<Element, () => void>();

/**
 * Whether an element's content is wider than the element, so it scrolls sideways. A region
 * that scrolls has to be focusable for keyboard users; one that does not should not be a
 * tab stop.
 */
export function useDocsOverflow(element: Ref<HTMLElement | undefined>) {
  const overflowing = ref(false);

  onMounted(() => {
    const target = element.value;
    if (!target) return;
    observer ??= new ResizeObserver((entries) => {
      for (const entry of entries) watched.get(entry.target)?.();
    });
    watched.set(target, () => (overflowing.value = target.scrollWidth > target.clientWidth + 1));
    observer.observe(target);
  });

  onBeforeUnmount(() => {
    if (!element.value) return;
    observer?.unobserve(element.value);
    watched.delete(element.value);
  });

  return overflowing;
}
