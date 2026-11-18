<script setup lang="ts">
import { computed } from "vue";
import MmBadge from "@mm/ui/components/MmBadge.vue";
import MmCard from "@mm/ui/components/MmCard.vue";
import MmEmpty from "@mm/ui/components/MmEmpty.vue";
import MmKv from "@mm/ui/components/MmKv.vue";
import MmKvItem from "@mm/ui/components/MmKvItem.vue";
import MmStat from "@mm/ui/components/MmStat.vue";
import MmTabs from "@mm/ui/components/MmTabs.vue";
import type { DashboardOperation, InstallationDashboard, ProjectDashboard } from "../api.generated";
import { duration, friendly, issueSummary, operationTone, plural, shareTone } from "../format";
import ControlPlane from "./ControlPlane.vue";
import DiagnosticList from "./DiagnosticList.vue";
import OperationList from "./OperationList.vue";
import OptionalRow from "./OptionalRow.vue";
import ServiceRow from "./ServiceRow.vue";

const props = defineProps<{ project: ProjectDashboard; installation: InstallationDashboard }>();

const services = computed(() => props.project.services);
const healthy = computed(() => services.value.filter((service) => service.health === "healthy").length);
// Non-breaking joins keep a count with its label when the line wraps.
const serviceDetail = computed(() =>
  (["degraded", "stopped", "unknown"] as const).reduce((detail, health) => {
    const count = services.value.filter((service) => service.health === health).length;
    return count ? `${detail}\u00a0· ${count}\u00a0${health}` : detail;
  }, "healthy"),
);
const routed = computed(() => services.value.filter((service) => service.routes.length).length);
// Optional units that are off on this machine, greyed out below the services they would add.
const offLocally = computed(() => (props.project.optional ?? []).filter((unit) => !unit.on));

// Shared services affect every project, so their findings are listed with the project's own.
const diagnostics = computed(() => [...props.project.diagnostics, ...props.installation.diagnostics]);
const issues = computed(() => issueSummary(diagnostics.value));

const latest = computed(() =>
  props.project.operations.reduce<DashboardOperation | undefined>(
    (newest, operation) =>
      newest && newest.requested_at_unix_ms >= operation.requested_at_unix_ms ? newest : operation,
    undefined,
  ),
);

const resources = computed(() =>
  (["volume", "network"] as const).map((kind) => ({
    kind,
    title: kind === "volume" ? "Volumes" : "Networks",
    names: props.project.resources.filter((resource) => resource.kind === kind).map((resource) => resource.logical_name),
  })),
);

const tabs = computed(() => [
  { value: "services", label: "Services", count: services.value.length },
  { value: "resources", label: "Data & networks", count: props.project.resources.length },
  { value: "activity", label: "Activity", count: props.project.operations.length },
  {
    value: "doctor",
    label: "Doctor",
    count: diagnostics.value.length,
    tone: issues.value.count ? issues.value.tone : undefined,
  },
]);
</script>

<template>
  <section class="overview" aria-labelledby="project-title">
    <header class="head">
      <h1 id="project-title" class="head__title">{{ project.name }}</h1>
      <ul class="head__tags" aria-label="Project context">
        <li>{{ project.environment }}</li>
        <li v-if="project.target !== project.environment">{{ project.target }}</li>
        <li v-if="project.runtime">{{ friendly(project.runtime) }}</li>
      </ul>
    </header>
    <div class="stats">
      <MmStat
        title="Services"
        :tone="shareTone(healthy, services.length)"
        :value="services.length ? `${healthy}/${services.length}` : 0"
        :detail="services.length ? serviceDetail : 'none reported'"
      />
      <MmStat
        title="Routes"
        :tone="routed ? 'ok' : 'idle'"
        :value="routed"
        :detail="`of ${plural(services.length, 'service')} routed`"
      />
      <MmStat title="Issues" :tone="issues.tone" :value="issues.count" :detail="issues.detail" />
      <MmStat
        v-if="latest"
        title="Last operation"
        :tone="operationTone[latest.state]"
        :value="latest.kind"
        :detail="duration(latest) ? `${latest.state} in ${duration(latest)}` : latest.state"
      />
      <MmStat v-else title="Last operation" tone="idle" value="none" detail="nothing recorded" />
    </div>
  </section>

  <MmTabs class="views" :tabs="tabs" label="Project dashboard sections">
    <template #services>
      <MmCard title="Services" :heading-level="2">
        <template #meta>{{ plural(services.length, "service") }}</template>
        <template v-if="services.length || offLocally.length">
          <div class="service service--head" aria-hidden="true">
            <span>State</span><span>Service</span><span>Replicas</span><span>Restarts</span><span>Routes</span>
          </div>
          <ul class="services">
            <ServiceRow v-for="service in services" :key="`${service.stack}/${service.name}`" :service="service" />
            <OptionalRow v-for="unit in offLocally" :key="`${unit.kind}/${unit.name}`" :unit="unit" />
          </ul>
        </template>
        <MmEmpty v-else title="No services">Start the stack with <code>mm dev up</code>.</MmEmpty>
      </MmCard>
    </template>

    <template #resources>
      <div class="pair">
        <MmCard v-for="group in resources" :key="group.kind" :title="group.title" :heading-level="2">
          <template #meta>{{ group.names.length }}</template>
          <ul v-if="group.names.length" class="names">
            <li v-for="name in group.names" :key="name">{{ name }}</li>
          </ul>
          <MmEmpty v-else :title="`No ${group.kind}s`" />
        </MmCard>
      </div>
    </template>

    <template #activity>
      <MmCard title="Operations" :heading-level="2">
        <template #meta>{{ project.operations.length }} recent</template>
        <OperationList v-if="project.operations.length" :operations="project.operations" />
        <MmEmpty v-else title="No activity">Builds, deploys and jobs appear here once they run.</MmEmpty>
      </MmCard>
    </template>

    <template #doctor>
      <MmCard title="Diagnostics" :heading-level="2">
        <template #meta>{{ plural(diagnostics.length, "finding") }}</template>
        <DiagnosticList v-if="diagnostics.length" :diagnostics="diagnostics" />
        <MmEmpty v-else tone="ok" title="No issues" />
      </MmCard>
    </template>
  </MmTabs>

  <div class="pair">
    <ControlPlane :installation="installation" />
    <MmCard title="Project" :heading-level="2">
      <MmKv>
        <MmKvItem label="Slug">{{ project.slug }}</MmKvItem>
        <MmKvItem label="Config">{{ project.config_revision }}</MmKvItem>
        <MmKvItem label="Runtime">
          <MmBadge :tone="project.runtime_available ? 'ok' : 'bad'">{{ project.runtime_available ? "available" : "unavailable" }}</MmBadge>
        </MmKvItem>
        <MmKvItem label="Env delivery">
          {{ project.environment_delivery.mode }}<span class="soft"> · {{ project.environment_delivery.provenance }}</span>
        </MmKvItem>
      </MmKv>
    </MmCard>
  </div>
</template>
