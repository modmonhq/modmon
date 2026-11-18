<template>
  <span class="mm-spinner" aria-hidden="true"><i /><i /><i /><i /><i /><i /></span>
</template>

<style>
/* A six-dot braille cell with the gap chasing round, like the terminal spinner. */
.mm-spinner {
  display: inline-grid;
  grid-template-columns: repeat(2, 0.16em);
  gap: 0.13em 0.14em;
  width: 1ch;
  justify-content: center;
  vertical-align: 0.02em;
  color: var(--mm-accent);
}

.mm-spinner i {
  width: 0.16em;
  height: 0.16em;
  background: currentColor;
  border-radius: 50%;
  animation: mm-spinner 0.8s steps(1) infinite;
}

.mm-spinner i:nth-child(1) {
  animation-delay: 0s;
}
.mm-spinner i:nth-child(2) {
  animation-delay: -0.667s;
}
.mm-spinner i:nth-child(4) {
  animation-delay: -0.533s;
}
.mm-spinner i:nth-child(6) {
  animation-delay: -0.4s;
}
.mm-spinner i:nth-child(5) {
  animation-delay: -0.267s;
}
.mm-spinner i:nth-child(3) {
  animation-delay: -0.133s;
}

@keyframes mm-spinner {
  0% {
    opacity: 0.15;
  }
  16.7% {
    opacity: 1;
  }
}
</style>
