<script setup lang="ts">
import MmBadge from "@mm/ui/components/MmBadge.vue";
import MmSpinner from "@mm/ui/components/MmSpinner.vue";
import type { DashboardOperation } from "../api.generated";
import { duration, formatTime, friendly, operationTone } from "../format";

defineProps<{ operations: readonly DashboardOperation[] }>();
</script>

<template>
  <ol class="operations">
    <li
      v-for="operation in operations"
      :key="`${operation.kind}-${operation.requested_at_unix_ms}`"
      class="operation"
      :data-tone="operationTone[operation.state]"
    >
      <span v-if="operation.state === 'running'" class="operation__state operation__state--running">
        <MmSpinner /><span>running</span>
      </span>
      <MmBadge v-else class="operation__state" :tone="operationTone[operation.state]">{{ operation.state }}</MmBadge>
      <strong class="operation__kind">{{ friendly(operation.kind) }}</strong>
      <code v-if="operation.error_code" class="operation__error">{{ operation.error_code }}</code>
      <span class="operation__when">
        <time :datetime="new Date(operation.requested_at_unix_ms).toISOString()">{{ formatTime(operation.requested_at_unix_ms) }}</time>
        <span v-if="duration(operation)" class="operation__duration">{{ duration(operation) }}</span>
      </span>
    </li>
  </ol>
</template>
