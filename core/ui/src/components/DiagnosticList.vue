<script setup lang="ts">
import MmCodeText from "@mm/ui/components/MmCodeText.vue";
import MmTone from "@mm/ui/components/MmTone.vue";
import type { DashboardDiagnostic } from "../api.generated";
import { severityTone } from "../format";

defineProps<{ diagnostics: readonly DashboardDiagnostic[] }>();
</script>

<template>
  <ul class="findings">
    <li
      v-for="(diagnostic, index) in diagnostics"
      :key="`${diagnostic.code}-${index}`"
      class="finding"
      :data-tone="severityTone[diagnostic.severity]"
    >
      <MmTone :tone="severityTone[diagnostic.severity]" />
      <div class="finding__body">
        <strong class="finding__summary"><span class="mm-sr-only">{{ diagnostic.severity }}: </span><MmCodeText :text="diagnostic.summary" /></strong>
        <code class="finding__code">{{ diagnostic.code }}</code>
        <p v-if="diagnostic.suggested_action" class="finding__action"><MmCodeText :text="diagnostic.suggested_action" /></p>
      </div>
    </li>
  </ul>
</template>
