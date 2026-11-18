<template>
  <div class="layout">
    <a class="site-skip" href="#main">Skip to content</a>
    <SiteNav />
    <main id="main">
      <slot />
    </main>
    <SiteFooter />
  </div>
</template>
