<script setup lang="ts">
import MmArrowLink from "@mm/ui/components/MmArrowLink.vue";
import MmBadge from "@mm/ui/components/MmBadge.vue";
import MmCard from "@mm/ui/components/MmCard.vue";
import MmEmpty from "@mm/ui/components/MmEmpty.vue";
import type { InstallationDashboard } from "../api.generated";
import { componentLabel, componentTone, host } from "../format";

defineProps<{ installation: InstallationDashboard }>();
</script>

<template>
  <MmCard class="control" title="Control plane" :heading-level="2">
    <template #meta>
      <MmBadge :tone="installation.installed ? 'ok' : 'warn'">{{ installation.installed ? "installed" : "not installed" }}</MmBadge>
    </template>
    <ul v-if="installation.components.length" class="control__components">
      <li v-for="component in installation.components" :key="component.kind" class="control__component">
        <MmBadge :tone="componentTone(component)">{{ componentLabel[component.kind] }}</MmBadge>
        <span class="control__version"><span class="mm-sr-only">version </span>{{ component.version }}</span>
      </li>
    </ul>
    <MmEmpty v-else title="No shared services installed">
      Run <code>mm dev setup</code> to install ingress, Mailpit and Cap.
    </MmEmpty>
    <p class="control__facts">
      <MmArrowLink :href="installation.route">{{ host(installation.route) }}</MmArrowLink>
      <span v-if="installation.stack_version">stack {{ installation.stack_version }}</span>
    </p>
  </MmCard>
</template>
