<script setup lang="ts">
import { computed } from "vue";
import MmArrowLink from "@mm/ui/components/MmArrowLink.vue";
import MmCard from "@mm/ui/components/MmCard.vue";
import MmEmpty from "@mm/ui/components/MmEmpty.vue";
import MmStat from "@mm/ui/components/MmStat.vue";
import type { DashboardProjectSummary, InstallationDashboard } from "../api.generated";
import { friendly, host, issueSummary, plural, shareTone } from "../format";
import ControlPlane from "./ControlPlane.vue";
import DiagnosticList from "./DiagnosticList.vue";

const props = defineProps<{ installation: InstallationDashboard; projects: DashboardProjectSummary[] }>();

const components = computed(() => props.installation.components);
const healthy = computed(() => components.value.filter((component) => component.healthy).length);
const issues = computed(() => issueSummary(props.installation.diagnostics));
</script>

<template>
  <section class="overview" aria-labelledby="installation-title">
    <header class="head">
      <h1 id="installation-title" class="head__title">Installation</h1>
      <ul class="head__tags" aria-label="Installation context">
        <li>{{ installation.runtime ? friendly(installation.runtime) : "runtime not selected" }}</li>
      </ul>
    </header>
    <div class="stats stats--three">
      <MmStat
        title="Projects"
        :tone="projects.length ? 'ok' : 'idle'"
        :value="projects.length"
        detail="registered"
      />
      <MmStat
        v-if="installation.installed"
        title="Control plane"
        :tone="shareTone(healthy, components.length)"
        :value="`${healthy}/${components.length}`"
        detail="shared services healthy"
      />
      <MmStat v-else title="Control plane" tone="warn" value="setup needed" detail="run mm dev setup" />
      <MmStat title="Issues" :tone="issues.tone" :value="issues.count" :detail="issues.detail" />
    </div>
  </section>

  <MmCard v-if="installation.diagnostics.length" title="Needs attention" :heading-level="2">
    <template #meta>{{ plural(installation.diagnostics.length, "finding") }}</template>
    <DiagnosticList :diagnostics="installation.diagnostics" />
  </MmCard>

  <div class="pair pair--lead">
    <MmCard title="Projects" :heading-level="2">
      <template #meta>{{ plural(projects.length, "project") }}</template>
      <ul v-if="projects.length" class="projects">
        <li v-for="item in projects" :key="item.project_id" class="project">
          <MmArrowLink class="project__link" :href="item.dashboard_url">{{ item.name }}</MmArrowLink>
          <span class="project__host">{{ host(item.dashboard_url) }}</span>
          <span class="project__environment">{{ item.environment }}</span>
        </li>
      </ul>
      <MmEmpty v-else title="No projects yet">Run <code>mm init</code> in a project to add it.</MmEmpty>
    </MmCard>
    <ControlPlane :installation="installation" />
  </div>
</template>
