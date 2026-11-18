<script setup lang="ts">
import { NuxtLink } from "#components";

const { site } = useAppConfig();
const route = useRoute();

const links = [
  { label: "Agents", to: "/#agents" },
  { label: "Cluster", to: "/#cluster" },
  { label: "Development", to: "/#development" },
  { label: "Docs", to: "/docs" },
];

// Only the docs are a section of their own; the other links are anchors on the home page.
const isCurrent = (to: string) => to === "/docs" && route.path.startsWith("/docs");

// The bar starts flush with the page and lifts into a floating capsule once you scroll.
const lifted = ref(false);
const open = ref(false);

function onScroll() {
  lifted.value = window.scrollY > 24;
}

onMounted(() => {
  onScroll();
  window.addEventListener("scroll", onScroll, { passive: true });
});
onBeforeUnmount(() => window.removeEventListener("scroll", onScroll));
watch(
  () => route.fullPath,
  () => (open.value = false),
);
</script>

<template>
  <header class="nav" :class="{ 'is-lifted': lifted || open, 'is-open': open }">
    <div class="nav__bar">
      <NuxtLink to="/" class="nav__brand" aria-label="mm home">
        <MmWordmark />
      </NuxtLink>
      <nav class="nav__links" aria-label="Primary">
        <NuxtLink
          v-for="link in links"
          :key="link.to"
          :to="link.to"
          class="nav__link"
          :class="{ 'is-current': isCurrent(link.to) }"
          :aria-current="isCurrent(link.to) ? 'page' : undefined"
        >
          {{ link.label }}
        </NuxtLink>
      </nav>
      <div class="nav__actions">
        <a :href="site.repo" class="nav__icon" aria-label="Source on GitHub" rel="noopener">
          <svg viewBox="0 0 16 16" width="18" height="18" aria-hidden="true">
            <path
              fill="currentColor"
              d="M8 .2a8 8 0 0 0-2.53 15.59c.4.07.55-.17.55-.38l-.01-1.49c-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.4 7.4 0 0 1 4 0c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48l-.01 2.2c0 .21.15.46.55.38A8 8 0 0 0 8 .2Z"
            />
          </svg>
        </a>
        <MmButton :as="NuxtLink" to="/#install" variant="primary" size="sm">
          Install
          <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <path
              d="m6 3.5 4.5 4.5L6 12.5"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </MmButton>
        <button
          type="button"
          class="nav__toggle"
          :aria-expanded="open"
          aria-controls="nav-sheet"
          @click="open = !open"
        >
          <span class="mm-sr-only">Menu</span>
          <i /><i />
        </button>
      </div>
    </div>
    <div id="nav-sheet" class="nav__sheet" :hidden="!open">
      <NuxtLink v-for="link in links" :key="link.to" :to="link.to" class="nav__sheet-link">
        {{ link.label }}
      </NuxtLink>
      <a :href="site.repo" class="nav__sheet-link" rel="noopener">GitHub</a>
    </div>
  </header>
</template>

<style scoped>
/*
 * The bar's controls are 2.125rem tall and sit --nav-gap inside its edge, so their radius
 * is the bar's radius less that gap.
 */
.nav {
  --nav-radius: 18px;
  --nav-gap: 9px;
  --nav-inner: calc(var(--nav-radius) - var(--nav-gap));

  position: fixed;
  inset: 0 0 auto;
  z-index: 50;
  padding: 0.6rem clamp(0.6rem, 2.4vw, 1.5rem) 0;
  pointer-events: none;
}

.nav__bar {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  max-width: calc(var(--site-max) + 2rem);
  height: calc(2.125rem + 2 * var(--nav-gap));
  margin-inline: auto;
  padding: 0 calc(var(--nav-gap) - 1px);
  pointer-events: auto;
  border: 1px solid transparent;
  border-radius: var(--nav-radius);
  transition:
    background-color 0.35s var(--mm-ease),
    border-color 0.35s var(--mm-ease),
    box-shadow 0.35s var(--mm-ease),
    backdrop-filter 0.35s var(--mm-ease);
}

.nav.is-lifted .nav__bar {
  background: rgb(10 13 11 / 72%);
  border-color: var(--mm-line-strong);
  box-shadow:
    inset 0 1px 0 rgb(255 255 255 / 5%),
    0 18px 50px -20px rgb(0 0 0 / 80%);
  backdrop-filter: blur(18px) saturate(150%);
}

.nav__brand {
  display: inline-flex;
  align-items: center;
  justify-self: start;
  height: 2.125rem;
  padding: 0 0.45rem;
  font-size: 1.3rem;
  text-decoration: none;
  border-radius: var(--nav-inner);
}

.nav__links {
  display: flex;
  gap: 0.25rem;
}

.nav__link {
  padding: 0.45rem 0.85rem;
  font-size: 0.9rem;
  font-weight: 500;
  color: var(--mm-ink-soft);
  text-decoration: none;
  border-radius: var(--nav-inner);
  transition:
    color 0.2s var(--mm-ease),
    background-color 0.2s var(--mm-ease);
}

.nav__link:hover {
  color: var(--mm-ink);
  background: rgb(255 255 255 / 6%);
}

.nav__link.is-current {
  color: var(--mm-ink);
  background: rgb(255 255 255 / 7%);
  box-shadow: inset 0 1px 0 rgb(255 255 255 / 6%);
}

.nav__actions {
  display: flex;
  gap: 0.5rem;
  align-items: center;
  justify-self: end;
}

.nav__icon {
  display: grid;
  place-items: center;
  width: 2.125rem;
  height: 2.125rem;
  color: var(--mm-ink-soft);
  border-radius: var(--nav-inner);
  transition:
    color 0.2s var(--mm-ease),
    background-color 0.2s var(--mm-ease);
}

.nav__icon:hover {
  color: var(--mm-ink);
  background: rgb(255 255 255 / 6%);
}

.nav__toggle {
  display: none;
  flex-direction: column;
  gap: 5px;
  align-items: center;
  justify-content: center;
  width: 2.125rem;
  height: 2.125rem;
  cursor: pointer;
  background: none;
  border: 0;
  border-radius: var(--nav-inner);
}

.nav__toggle i {
  width: 16px;
  height: 1.5px;
  background: var(--mm-ink);
  border-radius: 2px;
  transition: transform 0.25s var(--mm-ease);
}

.nav.is-open .nav__toggle i:first-of-type {
  transform: translateY(3.25px) rotate(45deg);
}

.nav.is-open .nav__toggle i:last-of-type {
  transform: translateY(-3.25px) rotate(-45deg);
}

.nav__sheet {
  display: none;
}

@media (max-width: 820px) {
  .nav__bar {
    grid-template-columns: 1fr auto;
  }

  .nav__links,
  .nav__icon {
    display: none;
  }

  .nav__toggle {
    display: flex;
  }

  .nav__sheet:not([hidden]) {
    display: grid;
    gap: 0.15rem;
    max-width: calc(var(--site-max) + 2rem);
    margin: 0.4rem auto 0;
    padding: calc(var(--nav-gap) - 1px);
    pointer-events: auto;
    background: rgb(10 13 11 / 92%);
    border: 1px solid var(--mm-line-strong);
    border-radius: var(--nav-radius);
    backdrop-filter: blur(18px);
  }

  .nav__sheet-link {
    padding: 0.8rem 0.9rem;
    font-weight: 500;
    color: var(--mm-ink);
    text-decoration: none;
    border-radius: var(--nav-inner);
  }

  .nav__sheet-link:hover {
    background: rgb(255 255 255 / 6%);
  }
}
</style>
