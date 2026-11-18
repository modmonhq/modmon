<script setup lang="ts">
import MmArrowLink from "@mm/ui/components/MmArrowLink.vue";
import MmBadge from "@mm/ui/components/MmBadge.vue";
import MmPips from "@mm/ui/components/MmPips.vue";
import type { DashboardService } from "../api.generated";
import { healthTone, host, provenanceLabel } from "../format";

defineProps<{ service: DashboardService }>();
</script>

<template>
  <li class="service">
    <div class="service__state">
      <MmBadge :tone="healthTone[service.health]">{{ service.health }}</MmBadge>
    </div>
    <h3 class="service__name">
      {{ service.name }} <span v-if="service.app" class="service__app">{{ service.app }}</span>
    </h3>
    <div class="service__replicas">
      <!-- The count beside the pips carries the same facts. -->
      <MmPips
        v-if="service.desired_count"
        aria-hidden="true"
        :desired="service.desired_count"
        :running="service.running_count"
        :healthy="service.healthy_count"
      />
      <span class="service__count">{{ service.running_count }}/{{ service.desired_count }}<span class="service__unit"> running</span></span>
    </div>
    <div class="service__restarts">
      <template v-if="service.restart_count">{{ service.restart_count }}<span class="service__unit">{{ service.restart_count === 1 ? " restart" : " restarts" }}</span></template>
    </div>
    <ul v-if="service.routes.length" class="service__routes">
      <li v-for="route in service.routes" :key="route.url" class="service__route">
        <MmArrowLink :href="route.url">{{ host(route.url) }}</MmArrowLink>
        <span class="service__source">{{ provenanceLabel[route.provenance] }}</span>
        <span v-if="!route.secure" class="service__insecure">no TLS</span>
      </li>
    </ul>
    <dl class="service__facts">
      <div><dt>stack</dt><dd>{{ service.stack }}</dd></div>
      <div v-if="service.image"><dt class="mm-sr-only">Image</dt><dd>{{ service.image }}</dd></div>
      <div v-if="service.revision"><dt class="mm-sr-only">Revision</dt><dd :title="service.revision">{{ service.revision.slice(0, 12) }}</dd></div>
    </dl>
    <dl v-if="service.exit_code != null || service.diagnostic_code" class="service__facts service__facts--problem" data-tone="bad">
      <div v-if="service.exit_code != null"><dt>exit</dt><dd>{{ service.exit_code }}</dd></div>
      <div v-if="service.diagnostic_code"><dt class="mm-sr-only">Diagnostic</dt><dd>{{ service.diagnostic_code }}</dd></div>
    </dl>
  </li>
</template>
