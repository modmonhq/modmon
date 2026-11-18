<script setup lang="ts">
import { computed } from "vue";
import MmBadge from "@mm/ui/components/MmBadge.vue";
import type { OptionalUnit } from "../api.generated";

const props = defineProps<{ unit: OptionalUnit }>();
const services = computed(() => props.unit.services ?? []);
const jobs = computed(() => props.unit.jobs ?? []);
</script>

<template>
  <li class="service service--off">
    <div class="service__state">
      <MmBadge tone="idle">off locally</MmBadge>
    </div>
    <h3 class="service__name">
      {{ unit.name }} <span class="service__app">optional {{ unit.kind }}</span>
    </h3>
    <dl v-if="services.length || jobs.length" class="service__facts">
      <div v-if="services.length"><dt>services</dt><dd>{{ services.join(", ") }}</dd></div>
      <div v-if="jobs.length"><dt>jobs</dt><dd>{{ jobs.join(", ") }}</dd></div>
    </dl>
    <p class="service__routes service__enable">
      <span class="service__source">turn on</span> <code>mm dev enable {{ unit.name }}</code>
    </p>
  </li>
</template>
