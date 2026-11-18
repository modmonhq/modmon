<script setup lang="ts">
import { NuxtLink } from "#components";

const demo = useTuiDemo();
const { vitality, autoplay, lastKey } = demo;

// The stage tilts back slightly and settles flat as the page scrolls.
const stage = ref<HTMLElement>();
const settle = ref(0);
let frame = 0;

function onScroll() {
  cancelAnimationFrame(frame);
  frame = requestAnimationFrame(() => {
    settle.value = Math.min(window.scrollY / 420, 1);
  });
}

onMounted(() => {
  onScroll();
  window.addEventListener("scroll", onScroll, { passive: true });
});
onBeforeUnmount(() => {
  window.removeEventListener("scroll", onScroll);
  cancelAnimationFrame(frame);
});
</script>

<template>
  <section id="top" class="hero" :style="{ '--vitality': vitality, '--settle': settle }">
    <div class="hero__backdrop" aria-hidden="true">
      <div class="hero__grid" />
      <div class="hero__beam" />
    </div>

    <div class="site-container hero__intro">
      <div class="hero__copy">
        <h1 class="display display--xl">
          <span class="ink-metal">Turn a list of servers</span><br />
          <span class="ink-metal">into </span><span class="ink-tone">one cluster.</span>
        </h1>
        <p class="lede">
          <code class="hero__mm">mm</code> is a Docker Swarm replacement that runs your Compose project across
          every host you list.
          <span class="hero__more">
            The nodes get an encrypted private network, service names that resolve on all of them, rolling
            deploys that roll back, HTTPS routes and autoscaling, shared volumes, and one merged log. Coding
            agents operate it within limits you set.
          </span>
        </p>
        <div class="hero__actions">
          <MmButton :as="NuxtLink" to="/docs/how-to/cluster" variant="primary" size="lg">
            Set up a cluster
            <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
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
          <MmButton :as="NuxtLink" to="/docs/how-to/from-docker-swarm" size="lg">
            Coming from Docker Swarm
          </MmButton>
        </div>
      </div>
      <HomeNodeList class="hero__config hero__config--beside" />
    </div>

    <div class="hero__stage-wrap">
      <div ref="stage" class="hero__stage">
        <div class="hero__glow" aria-hidden="true" />
        <MmWindow class="hero__window">
          <template #title>mm -e production · ~/code/acme</template>
          <template #aside>
            <span class="hero__mode" :class="{ 'is-live': !autoplay }">
              <i />{{ autoplay ? "tour" : "you're driving" }}
            </span>
          </template>
          <TuiDemo :demo="demo" />
        </MmWindow>
      </div>
      <p class="hero__hint">
        <span class="hero__hint-lead">This terminal is interactive. Click it, then press</span>
        <span class="hero__keys">
          <kbd :class="{ 'is-hit': lastKey === 'u' }">u</kbd>
          <kbd :class="{ 'is-hit': lastKey === 'Enter' }">enter</kbd>
          <kbd :class="{ 'is-hit': lastKey === 'j' || lastKey === 'k' }">j</kbd>
          <kbd :class="{ 'is-hit': lastKey === 'l' }">l</kbd>
          <kbd :class="{ 'is-hit': lastKey === '2' }">2</kbd>
          <kbd :class="{ 'is-hit': lastKey === '?' }">?</kbd>
        </span>
      </p>
    </div>

    <!-- On a phone the terminal comes first and the node list follows it. -->
    <div class="site-container">
      <HomeNodeList class="hero__config hero__config--below" />
    </div>
  </section>
</template>

<style scoped>
.hero {
  position: relative;
  isolation: isolate;
  padding-top: calc(var(--site-nav-h) + clamp(1.75rem, 3.6vw, 3rem));
  overflow-x: clip;
}

/* Backdrop: a dot field that fades out, and a soft beam falling onto the terminal. */
.hero__backdrop {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
}

.hero__grid {
  position: absolute;
  inset: 0;
  background-image: radial-gradient(rgb(232 246 214 / 17%) 1px, transparent 1.3px);
  background-size: 26px 26px;
  mask-image: radial-gradient(ellipse 85% 70% at 50% 0%, #000 0%, transparent 80%);
}

.hero__beam {
  position: absolute;
  top: -22rem;
  left: 50%;
  width: min(120rem, 180vw);
  height: 62rem;
  background:
    radial-gradient(ellipse 34% 50% at 50% 42%, rgb(var(--mm-accent-rgb) / 13%), transparent 70%),
    radial-gradient(ellipse 60% 44% at 50% 30%, rgb(143 211 255 / 5%), transparent 70%);
  opacity: calc(0.45 + var(--vitality) * 0.55);
  transform: translateX(-50%);
  transition: opacity 0.8s var(--mm-ease);
}

.hero__intro {
  display: grid;
  grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr);
  gap: clamp(1.5rem, 4vw, 4.5rem);
  align-items: end;
}

.hero__copy {
  display: grid;
  gap: clamp(1.1rem, 2vw, 1.6rem);
  justify-items: start;
}

.hero__copy .lede {
  max-width: 37rem;
}

.hero__mm {
  padding: 0.05em 0.4em;
  font-size: 0.92em;
  font-weight: 600;
  color: var(--mm-accent);
  background: rgb(var(--mm-accent-rgb) / 10%);
  border: 1px solid rgb(var(--mm-accent-rgb) / 20%);
  border-radius: 5px;
}

.hero__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.hero__config--below {
  display: none;
  margin-top: 2rem;
}

/* The stage */
.hero__stage-wrap {
  max-width: calc(var(--site-max) + 2 * var(--site-gutter) + 5rem);
  margin: clamp(2.25rem, 3.8vw, 3.25rem) auto 0;
  padding-inline: clamp(0.5rem, 2.5vw, 2.5rem);
  perspective: 2400px;
  perspective-origin: 50% 0%;
}

.hero__stage {
  position: relative;
  transform: rotateX(calc((1 - var(--settle)) * 7deg)) scale(calc(0.985 + var(--settle) * 0.015));
  transform-origin: 50% 0%;
  will-change: transform;
}

.hero__glow {
  position: absolute;
  inset: -10% -8% -22%;
  z-index: -1;
  background:
    radial-gradient(ellipse 46% 40% at 50% 52%, rgb(var(--mm-accent-rgb) / 30%), transparent 72%),
    radial-gradient(ellipse 52% 20% at 50% 80%, rgb(var(--mm-accent-rgb) / 16%), transparent 72%);
  filter: blur(46px);
  pointer-events: none;
  opacity: calc(0.18 + var(--vitality) * 0.82);
  transition: opacity 0.9s var(--mm-ease);
}

.hero__window {
  --mm-window-bg: #080a09;
  --tui-bg: #080a09;

  box-shadow:
    0 0 0 1px rgb(232 246 214 / 16%),
    inset 0 1px 0 rgb(255 255 255 / 9%),
    0 2px 0 rgb(0 0 0 / 60%),
    0 60px 120px -50px rgb(0 0 0 / 90%),
    0 30px 90px -30px rgb(var(--mm-accent-rgb) / calc(0.04 + var(--vitality) * 0.16));
  transition: box-shadow 0.9s var(--mm-ease);
}

/* A hairline of light along the top edge, brightest in the middle. */
.hero__window::before {
  position: absolute;
  inset: -1px 12% auto;
  z-index: 1;
  height: 1px;
  pointer-events: none;
  content: "";
  background: linear-gradient(90deg, transparent, rgb(var(--mm-accent-rgb) / 80%), transparent);
  opacity: calc(0.25 + var(--vitality) * 0.75);
  transition: opacity 0.9s var(--mm-ease);
}

.hero__window :deep(.mm-window__body) {
  padding: clamp(0.5rem, 1.1vw, 0.9rem) clamp(0.5rem, 1.2vw, 1rem) clamp(0.45rem, 1vw, 0.8rem);
}

.hero__window :deep(.tui:focus-visible) {
  outline: none;
}

.hero__window:has(.tui:focus-visible) {
  box-shadow:
    0 0 0 1px rgb(var(--mm-accent-rgb) / 70%),
    0 0 0 4px rgb(var(--mm-accent-rgb) / 14%),
    0 60px 120px -50px rgb(0 0 0 / 90%),
    0 30px 90px -30px rgb(var(--mm-accent-rgb) / 20%);
}

.hero__mode {
  display: inline-flex;
  gap: 0.5rem;
  align-items: center;
  font-family: var(--mm-font-mono);
  font-size: 0.72rem;
  color: var(--mm-muted);
  white-space: nowrap;
}

.hero__mode i {
  width: 6px;
  height: 6px;
  background: var(--mm-muted);
  border-radius: 50%;
}

.hero__mode.is-live {
  color: var(--mm-accent);
}

.hero__mode.is-live i {
  background: var(--mm-accent);
  box-shadow: 0 0 8px rgb(var(--mm-accent-rgb) / 80%);
}

.hero__hint {
  display: flex;
  flex-wrap: wrap;
  gap: 0.6rem 0.9rem;
  align-items: center;
  justify-content: center;
  margin: 1.75rem 0 0;
  font-size: 0.875rem;
  color: var(--mm-muted);
}

.hero__keys {
  display: inline-flex;
  gap: 0.35rem;
}

.hero__keys kbd {
  display: grid;
  place-items: center;
  min-width: 1.7rem;
  height: 1.7rem;
  padding: 0 0.4rem;
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--mm-ink-soft);
  background: linear-gradient(180deg, rgb(255 255 255 / 9%), rgb(255 255 255 / 3%));
  border: 1px solid var(--mm-line-strong);
  border-bottom-width: 2px;
  border-radius: 6px;
  transition:
    color 0.15s var(--mm-ease),
    border-color 0.15s var(--mm-ease),
    transform 0.15s var(--mm-ease),
    box-shadow 0.15s var(--mm-ease);
}

.hero__keys kbd.is-hit {
  color: var(--mm-accent-ink);
  background: var(--mm-accent);
  border-color: var(--mm-accent);
  box-shadow: 0 0 18px rgb(var(--mm-accent-rgb) / 60%);
  transform: translateY(1px);
}

/* Entrance: the copy rises in sequence, then the terminal lifts into its light. */
.hero__copy > *,
.hero__config--beside {
  animation: hero-rise 0.9s var(--mm-ease-out) both;
}

.hero__copy > :nth-child(2) {
  animation-delay: 0.1s;
}

.hero__copy > :nth-child(3) {
  animation-delay: 0.18s;
}

.hero__config--beside {
  animation-delay: 0.26s;
}

.hero__stage-wrap {
  animation: hero-lift 1.3s 0.2s var(--mm-ease-out) both;
}

@keyframes hero-rise {
  from {
    opacity: 0;
    transform: translateY(14px);
  }
}

@keyframes hero-lift {
  from {
    opacity: 0;
    transform: translateY(48px);
  }
}

@media (max-width: 900px) {
  .hero__intro {
    grid-template-columns: minmax(0, 1fr);
    gap: 1.75rem;
  }
}

@media (max-width: 760px) {
  /* Keep the terminal near the top on phones: one sentence above it, the node list below. */
  .hero__more,
  .hero__config--beside,
  .hero__window :deep(.mm-window__title) {
    display: none;
  }

  .hero__config--below {
    display: block;
  }

  .hero__hint-lead {
    width: 100%;
    text-align: center;
  }
}

@media (prefers-reduced-motion: reduce) {
  .hero__stage {
    transform: none;
  }

  .hero__copy > *,
  .hero__config--beside,
  .hero__stage-wrap {
    animation: none;
  }
}
</style>
