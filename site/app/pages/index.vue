<script setup lang="ts">
const { site } = useAppConfig();

useSeoMeta({
  title: "mm: turn a list of servers into one cluster",
  description:
    "mm is a Docker Swarm replacement that runs your Docker Compose project across every host you list: a private network, service names on every node, rolling deploys with rollback, HTTPS routes, autoscaling, shared volumes, and one merged log.",
  ogTitle: "mm",
  ogDescription: site.description,
  ogImage: `${site.url}/og.png`,
  twitterCard: "summary_large_image",
});
</script>

<template>
  <div>
    <HomeHero />
    <HomeGains />
    <!-- Below the fold: rendered on the server, hydrated only when scrolled into view. -->
    <LazyHomeAgents hydrate-on-visible />
    <LazyHomeCluster hydrate-on-visible />
    <LazyHomeFeatures hydrate-on-visible />
    <LazyHomeInstall hydrate-on-visible />
  </div>
</template>
