<script setup lang="ts">
import { nextTick } from "vue";
import { useRouter } from "vue-router";

import { useAppStore } from "../stores/app";
import { useRemoteMediaHealthStore } from "../stores/remoteMediaHealth";

const app = useAppStore();
const health = useRemoteMediaHealthStore();
const router = useRouter();

async function openSettings() {
  await router.push({ path: "/settings", hash: "#network-proxy" });
  await nextTick();
  document.getElementById("network-proxy")?.focus();
}

async function retry() {
  health.retryAll();
  await nextTick();
  document.getElementById("app-main-content")?.focus();
}

async function dismiss() {
  health.dismiss();
  await nextTick();
  document.getElementById("app-main-content")?.focus();
}
</script>

<template>
  <div
    data-testid="remote-media-status"
    role="status"
    aria-live="polite"
    aria-atomic="true"
  >
    <aside
      v-if="health.bannerVisible"
      data-testid="remote-media-failure-banner"
      class="flex shrink-0 flex-col gap-3 border-b border-warn/30 bg-warn/10 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
    >
      <div>
        <h2 class="text-sm font-semibold text-warn">Instagram previews are unavailable</h2>
        <p class="mt-1 text-xs text-slate-300">
          <template v-if="app.hasProxy">
            Check the configured proxy or try a VPN, then retry previews.
          </template>
          <template v-else>
            Your network may be blocking Instagram media. Turn on a VPN or configure a proxy in Settings.
          </template>
        </p>
      </div>
      <div class="flex shrink-0 flex-wrap gap-2">
        <button type="button" class="btn-secondary" @click="openSettings">Open Settings</button>
        <button type="button" class="btn-primary" @click="retry">Retry</button>
        <button type="button" class="btn-secondary" @click="dismiss">Dismiss</button>
      </div>
    </aside>
  </div>
</template>
