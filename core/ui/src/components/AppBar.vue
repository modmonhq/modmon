<script setup lang="ts">
import MmBadge from "@mm/ui/components/MmBadge.vue";
import MmWordmark from "@mm/ui/components/MmWordmark.vue";
import type { DashboardProjectSummary } from "../api.generated";

defineProps<{
  home: string;
  projects: DashboardProjectSummary[];
  current?: string;
  stale: boolean;
  controlPlaneDegraded: boolean;
}>();
</script>

<template>
  <header class="bar">
    <div class="bar__inner">
      <a class="bar__brand" :href="home" aria-label="mm installation dashboard"><MmWordmark /></a>
      <nav v-if="projects.length" class="bar__nav" aria-label="Registered projects">
        <a
          v-for="item in projects"
          :key="item.project_id"
          class="bar__link"
          :href="item.dashboard_url"
          :aria-current="current === item.project_id ? 'page' : undefined"
        >{{ item.name }}</a>
      </nav>
      <div class="bar__status">
        <MmBadge v-if="controlPlaneDegraded" tone="warn">control plane degraded</MmBadge>
        <MmBadge role="status" :tone="stale ? 'warn' : 'ok'" :pulse="!stale">{{ stale ? "reconnecting" : "live" }}</MmBadge>
      </div>
    </div>
  </header>
</template>
