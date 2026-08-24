<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useAppStore } from "./stores/app";
import { useJobsStore } from "./stores/jobs";
import { formatBalance } from "./lib/ipc";

const app = useAppStore();
const jobs = useJobsStore();
const route = useRoute();
const router = useRouter();

onMounted(async () => {
  await app.init();
  jobs.init().catch(() => {});
  if (!app.hasToken && route.path !== "/onboarding") {
    router.push("/onboarding");
  }
});

const showChrome = computed(() => route.path !== "/onboarding");
</script>

<template>
  <div v-if="!app.ready" class="flex h-full items-center justify-center text-slate-500">
    Loading…
  </div>
  <div v-else class="flex h-full flex-col">
    <header
      v-if="showChrome"
      class="flex min-w-0 flex-col items-stretch gap-2 border-b border-line bg-surface-1 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
    >
      <div class="flex shrink-0 items-baseline gap-2 select-none">
        <span
          class="bg-gradient-to-r from-[var(--color-accent-2)] to-[var(--color-accent)] bg-clip-text text-lg font-bold text-transparent"
          >insta-dl-gui</span
        >
      </div>
      <nav
        aria-label="Primary"
        class="flex w-full min-w-0 max-w-full items-center gap-1 overflow-x-auto pb-1 sm:w-auto sm:overflow-visible sm:pb-0"
      >
        <RouterLink
          v-for="r in [
            { path: '/download', label: 'Download' },
            { path: '/explore', label: 'Explore' },
            { path: '/library', label: 'Library' },
            { path: '/queue', label: 'Queue' },
            { path: '/settings', label: 'Settings' },
          ]"
          :key="r.path"
          :to="r.path"
          class="shrink-0 rounded-lg px-3 py-1.5 text-sm text-slate-400 hover:bg-surface-3 hover:text-slate-200"
          active-class="!text-slate-100 bg-surface-3"
        >
          {{ r.label }}
        </RouterLink>
        <button
          class="ml-2 shrink-0 rounded-full border border-line px-3 py-1 text-xs tabular-nums text-slate-300 hover:bg-surface-3 sm:ml-3"
          title="HikerAPI balance — click to refresh"
          @click="app.refreshBalance()"
        >
          <template v-if="app.balance">{{ formatBalance(app.balance) }}</template>
          <template v-else>— req</template>
        </button>
      </nav>
    </header>
    <main class="min-h-0 flex-1 overflow-y-auto">
      <RouterView />
    </main>
  </div>
</template>
