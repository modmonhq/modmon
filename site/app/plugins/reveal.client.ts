/*
 * Scroll reveal for elements marked `data-reveal`. Without JavaScript, or with reduced
 * motion, everything is simply visible: the hiding styles only apply under `.js-reveal`.
 *
 * Elements are found by watching the DOM rather than by route hooks, because pages and
 * their lazily loaded sections are inserted at different times after a client-side
 * navigation, and anything missed would stay hidden.
 */
export default defineNuxtPlugin(() => {
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;

  const visibility = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        // An attribute, not a class: Vue rewrites `class` when a component re-renders.
        entry.target.setAttribute("data-in", "");
        visibility.unobserve(entry.target);
      }
    },
    { rootMargin: "0px 0px -8% 0px", threshold: 0.08 },
  );

  let queued = false;

  function scan() {
    queued = false;
    for (const element of document.querySelectorAll("[data-reveal]:not([data-in])")) {
      visibility.observe(element);
    }
  }

  function queue() {
    if (queued) return;
    queued = true;
    requestAnimationFrame(scan);
  }

  document.documentElement.classList.add("js-reveal");
  new MutationObserver(queue).observe(document.body, { childList: true, subtree: true });
  queue();
});
