<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import DashboardPage from "./components/DashboardPage.vue";
import StatePanel from "./components/StatePanel.vue";
import { isDashboardEvent, loadDashboard } from "./api";
import type { DashboardResponse } from "./api.generated";

const data = ref<DashboardResponse>();
const loading = ref(true);
const stale = ref(false);
const error = ref<string>();
let events: EventSource | undefined;
let disconnectTimer: ReturnType<typeof setTimeout> | undefined;
let missedUpdates = false;
let request: AbortController | undefined;

// EventSource retries on its own; only a drop that outlasts this grace period is worth showing.
const reconnectGrace = 5_000;

function markConnected() {
  stale.value = false;
  clearTimeout(disconnectTimer);
  disconnectTimer = undefined;
}

function markDisconnected() {
  missedUpdates = true;
  if (stale.value || disconnectTimer) return;
  disconnectTimer = setTimeout(() => {
    disconnectTimer = undefined;
    stale.value = true;
  }, reconnectGrace);
}

function dashboardEvent(event: MessageEvent<string>, type: "connected" | "refresh") {
  try {
    const update: unknown = JSON.parse(event.data);
    if (isDashboardEvent(update) && update.type === type) return update;
  } catch {
    // Invalid stream data is handled as a disconnected snapshot below.
  }
  stale.value = true;
  return undefined;
}

async function refresh() {
  request?.abort();
  request = new AbortController();
  try {
    data.value = await loadDashboard(request.signal);
    error.value = undefined;
  } catch (reason) {
    if (reason instanceof DOMException && reason.name === "AbortError") return;
    error.value = reason instanceof Error ? reason.message : "The local dashboard is unavailable.";
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void refresh();
  events = new EventSource("/api/v1/events", { withCredentials: true });
  events.addEventListener("connected", (event) => {
    if (!dashboardEvent(event as MessageEvent<string>, "connected")) return;
    markConnected();
    if (missedUpdates) {
      missedUpdates = false;
      void refresh();
    }
  });
  events.addEventListener("refresh", (event) => {
    if (dashboardEvent(event as MessageEvent<string>, "refresh")) {
      markConnected();
      void refresh();
    }
  });
  events.onerror = markDisconnected;
});

onBeforeUnmount(() => {
  request?.abort();
  events?.close();
  clearTimeout(disconnectTimer);
});
</script>

<template>
  <StatePanel v-if="loading" state="loading" />
  <StatePanel v-else-if="error && !data" state="error" :detail="error" @retry="refresh" />
  <DashboardPage v-else-if="data" :dashboard="data" :stale="stale" :refresh-error="error" />
</template>
