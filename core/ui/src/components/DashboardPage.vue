<script setup lang="ts">
import { computed } from "vue";
import MmCodeText from "@mm/ui/components/MmCodeText.vue";
import MmNotice from "@mm/ui/components/MmNotice.vue";
import type { DashboardResponse } from "../api.generated";
import { formatClock } from "../format";
import AppBar from "./AppBar.vue";
import InstallationView from "./InstallationView.vue";
import ProjectView from "./ProjectView.vue";

const props = defineProps<{ dashboard: DashboardResponse; stale: boolean; refreshError?: string }>();

const controlPlaneDegraded = computed(() => props.dashboard.installation.components.some((item) => !item.healthy));
const snapshotTime = computed(() => new Date(props.dashboard.generated_at_unix_ms).toISOString());
const snapshotClock = computed(() => formatClock(props.dashboard.generated_at_unix_ms));
</script>

<template>
  <div class="dash">
    <AppBar
      :home="dashboard.installation.route"
      :projects="dashboard.projects"
      :current="dashboard.project?.project_id"
      :stale="stale"
      :control-plane-degraded="controlPlaneDegraded"
    />

    <main id="main" class="dash__main">
      <MmNotice v-if="refreshError" role="alert" tone="warn" title="Refresh failed">
        <MmCodeText class="dash__reason" :text="refreshError" /> <span>Showing the snapshot from <time :datetime="snapshotTime">{{ snapshotClock }}</time>.</span>
      </MmNotice>
      <MmNotice v-else-if="stale" tone="warn" title="Connection to mm lost">
        Showing the snapshot from <time :datetime="snapshotTime">{{ snapshotClock }}</time> until it reconnects.
      </MmNotice>

      <ProjectView v-if="dashboard.project" :project="dashboard.project" :installation="dashboard.installation" />
      <InstallationView v-else :installation="dashboard.installation" :projects="dashboard.projects" />
    </main>

    <footer class="dash__foot">
      <span>mm local dashboard</span>
      <span>snapshot from <time :datetime="snapshotTime">{{ snapshotClock }}</time></span>
    </footer>
  </div>
</template>
